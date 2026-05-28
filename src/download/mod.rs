pub mod assets;
pub mod java;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Semaphore};

use crate::error::AppError;
use crate::system;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadJob {
    pub id: String,
    pub url: String,
    pub destination_path: PathBuf,
    pub expected_sha1: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub job_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(DownloadProgress),
    Complete { job_id: String },
    Failed { job_id: String, reason: String },
    Speed { bytes_per_second: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadControl {
    Run,
    Pause,
    Cancel,
}

const MAX_RETRIES: u32 = 4;
const RETRY_BASE_MS: u64 = 800;

#[derive(Clone)]
pub struct DownloadManager {
    client: reqwest::Client,
    limit: Arc<Semaphore>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            limit: Arc::new(Semaphore::new(8)),
        }
    }

    pub async fn run_queue(
        &self,
        jobs: Vec<DownloadJob>,
        tx: mpsc::Sender<DownloadEvent>,
        control: watch::Receiver<DownloadControl>,
    ) -> Result<(), AppError> {
        let mut handles = Vec::new();
        for job in jobs {
            let permit = self
                .limit
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| AppError::Download(e.to_string()))?;
            let client = self.client.clone();
            let tx = tx.clone();
            let control = control.clone();
            handles.push(tokio::spawn(async move {
                let permit_guard = permit;
                let result = download_one(client, job.clone(), tx.clone(), control).await;
                drop(permit_guard);
                if let Err(error) = result {
                    let _ = tx
                        .send(DownloadEvent::Failed {
                            job_id: job.id,
                            reason: error.to_string(),
                        })
                        .await;
                }
            }));
        }
        for handle in handles {
            handle
                .await
                .map_err(|e| AppError::Download(e.to_string()))?;
        }
        Ok(())
    }
}

pub async fn download_jobs_checked(jobs: Vec<DownloadJob>) -> Result<(), AppError> {
    download_jobs_checked_with_progress(jobs, None).await
}

pub async fn download_jobs_checked_with_progress(
    jobs: Vec<DownloadJob>,
    progress_tx: Option<mpsc::UnboundedSender<(usize, usize)>>,
) -> Result<(), AppError> {
    let (control_tx, control_rx) = watch::channel(DownloadControl::Run);
    let result =
        download_jobs_checked_with_progress_and_control(jobs, progress_tx, control_rx).await;
    drop(control_tx);
    result
}

pub async fn download_jobs_checked_with_progress_and_control(
    jobs: Vec<DownloadJob>,
    progress_tx: Option<mpsc::UnboundedSender<(usize, usize)>>,
    control_rx: watch::Receiver<DownloadControl>,
) -> Result<(), AppError> {
    let client = reqwest::Client::new();
    let limit = Arc::new(Semaphore::new(16));
    let (event_tx, mut event_rx) = mpsc::channel::<DownloadEvent>(1024);
    let drain = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let total = jobs.len();
    let mut completed = 0usize;
    let mut handles = Vec::new();
    for job in jobs.iter() {
        if job_is_valid(job).await? {
            completed += 1;
            if let Some(tx) = &progress_tx {
                let _ = tx.send((completed, total));
            }
            continue;
        }

        let job = job.clone();
        let permit = limit
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| AppError::Download(e.to_string()))?;
        let client = client.clone();
        let tx = event_tx.clone();
        let control = control_rx.clone();
        handles.push(tokio::spawn(async move {
            let permit_guard = permit;
            let job_id = job.id.clone();
            let job_url = job.url.clone();
            let result = download_one(client, job, tx, control)
                .await
                .map_err(|error| AppError::Download(format!("{job_id} from {job_url}: {error}")));
            drop(permit_guard);
            result
        }));
    }

    drop(event_tx);

    let mut first_error = None;
    for handle in handles {
        match handle
            .await
            .map_err(|e| AppError::Download(e.to_string()))?
        {
            Ok(()) => {
                completed += 1;
                if let Some(tx) = &progress_tx {
                    let _ = tx.send((completed, total));
                }
            }
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    let _ = drain.await;

    if let Some(error) = first_error {
        cleanup_jobs(&jobs).await;
        Err(error)
    } else {
        Ok(())
    }
}

async fn cleanup_jobs(jobs: &[DownloadJob]) {
    for job in jobs {
        cleanup_job_files(job).await;
    }
}

async fn cleanup_job_files(job: &DownloadJob) {
    let temp_path = job.destination_path.with_extension("part");
    let _ = tokio::fs::remove_file(&temp_path).await;
    if tokio::fs::metadata(&job.destination_path).await.is_ok() {
        if let Some(expected) = &job.expected_sha1 {
            if let Ok(actual) = assets::sha1_file(&job.destination_path).await {
                if actual != *expected {
                    let _ = tokio::fs::remove_file(&job.destination_path).await;
                }
                return;
            }
        }
    }
}

async fn job_is_valid(job: &DownloadJob) -> Result<bool, AppError> {
    if tokio::fs::metadata(&job.destination_path).await.is_err() {
        return Ok(false);
    }

    if let Some(expected_sha1) = &job.expected_sha1 {
        let actual = assets::sha1_file(&job.destination_path).await?;
        return Ok(&actual == expected_sha1);
    }

    Ok(true)
}

async fn download_one(
    client: reqwest::Client,
    job: DownloadJob,
    tx: mpsc::Sender<DownloadEvent>,
    mut control: watch::Receiver<DownloadControl>,
) -> Result<(), AppError> {
    if let Some(parent) = job.destination_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let required = job.size_bytes.unwrap_or(8 * 1024 * 1024);
    system::ensure_disk_space(&job.destination_path, required).await?;

    let temp_path = job.destination_path.with_extension("part");
    let mut attempt = 0u32;
    loop {
        match download_one_attempt(&client, &job, &temp_path, &tx, &mut control).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                if matches!(error, AppError::Download(ref reason) if reason == "download cancelled")
                {
                    cleanup_job_files(&job).await;
                    return Err(error);
                }
                attempt += 1;
                if attempt >= MAX_RETRIES {
                    cleanup_job_files(&job).await;
                    return Err(error);
                }
                let delay = RETRY_BASE_MS * 2u64.pow(attempt.saturating_sub(1));
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }
    }
}

async fn download_one_attempt(
    client: &reqwest::Client,
    job: &DownloadJob,
    temp_path: &PathBuf,
    tx: &mpsc::Sender<DownloadEvent>,
    control: &mut watch::Receiver<DownloadControl>,
) -> Result<(), AppError> {
    let existing = tokio::fs::metadata(temp_path)
        .await
        .ok()
        .map(|meta| meta.len())
        .unwrap_or(0);
    let mut request = client.get(&job.url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let mut response = request.send().await?.error_for_status()?;
    let total = job
        .size_bytes
        .or_else(|| response.content_length())
        .map(|size| size.saturating_add(existing));

    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = tokio::fs::remove_file(temp_path).await;
        response = client.get(&job.url).send().await?.error_for_status()?;
    } else if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        // resume into existing .part
    } else if existing > 0 {
        let _ = tokio::fs::remove_file(temp_path).await;
    }

    let mut file = if existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(temp_path)
            .await?;
        file.seek(std::io::SeekFrom::End(0)).await?;
        file
    } else {
        tokio::fs::File::create(temp_path).await?
    };

    let mut downloaded = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        existing
    } else {
        0
    };
    let mut last_tick = Instant::now();
    let mut bytes_since_tick = 0u64;

    loop {
        let control_state = { *control.borrow() };
        match control_state {
            DownloadControl::Cancel => {
                drop(file);
                return Err(AppError::Download("download cancelled".into()));
            }
            DownloadControl::Pause => {
                control
                    .changed()
                    .await
                    .map_err(|e| AppError::Download(e.to_string()))?;
                continue;
            }
            DownloadControl::Run => {}
        }

        let Some(chunk) = response.chunk().await? else {
            break;
        };
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        bytes_since_tick += chunk.len() as u64;
        let progress = DownloadProgress {
            job_id: job.id.clone(),
            downloaded_bytes: downloaded,
            total_bytes: total,
        };
        let _ = tx.send(DownloadEvent::Progress(progress)).await;
        if last_tick.elapsed() >= Duration::from_millis(500) {
            let speed = bytes_since_tick * 1000 / last_tick.elapsed().as_millis().max(1) as u64;
            let _ = tx
                .send(DownloadEvent::Speed {
                    bytes_per_second: speed,
                })
                .await;
            last_tick = Instant::now();
            bytes_since_tick = 0;
        }
    }
    file.flush().await?;
    drop(file);

    if let Some(expected) = &job.expected_sha1 {
        let actual = assets::sha1_file(temp_path).await?;
        if actual != *expected {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(AppError::Download(format!(
                "sha1 mismatch: expected {expected}, got {actual}"
            )));
        }
    }

    let _ = tokio::fs::remove_file(&job.destination_path).await;
    tokio::fs::rename(temp_path, &job.destination_path).await?;
    let _ = tx
        .send(DownloadEvent::Complete {
            job_id: job.id.clone(),
        })
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_test_server(
        routes: HashMap<String, Vec<u8>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let handle = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let mut buf = [0u8; 2048];
                let Ok(n) = socket.read(&mut buf).await else {
                    continue;
                };
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let path = path.split('?').next().unwrap_or("/");
                if let Some(body) = routes.get(path) {
                    let response =
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                } else {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });
        (format!("http://{}", addr), handle)
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("swift-launcher-test-{prefix}-{pid}-{now}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn downloader_sends_progress_and_writes_file() {
        let mut routes = HashMap::new();
        routes.insert("/file.bin".to_string(), b"hello-world".to_vec());
        let (base, handle) = spawn_test_server(routes).await;

        let target_dir = temp_dir("download");
        let destination = target_dir.join("file.bin");

        let jobs = vec![DownloadJob {
            id: "job-1".to_string(),
            url: format!("{base}/file.bin"),
            destination_path: destination.clone(),
            expected_sha1: None,
            size_bytes: None,
        }];

        let (tx, mut rx) = mpsc::channel::<DownloadEvent>(16);
        let (control_tx, control_rx) = watch::channel(DownloadControl::Run);
        let manager = DownloadManager::new();
        let task = tokio::spawn(async move { manager.run_queue(jobs, tx, control_rx).await });

        drop(control_tx);

        let mut saw_progress = false;
        let mut saw_complete = false;
        while let Some(event) = rx.recv().await {
            match event {
                DownloadEvent::Progress(_) => saw_progress = true,
                DownloadEvent::Complete { .. } => {
                    saw_complete = true;
                    break;
                }
                DownloadEvent::Failed { reason, .. } => panic!("download failed: {reason}"),
                DownloadEvent::Speed { .. } => {}
            }
        }

        let result = task.await.unwrap();
        handle.abort();

        assert!(result.is_ok());
        assert!(saw_progress);
        assert!(saw_complete);
        let bytes = tokio::fs::read(&destination).await.unwrap();
        assert_eq!(bytes, b"hello-world");
    }
}
