use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::widget::markdown;
use iced::widget::scrollable::{self, Id as ScrollableId, RelativeOffset};
use iced::{stream, time, Element, Length, Subscription, Task};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::auth::{AuthProvider, Session};
use crate::download::DownloadControl;
use crate::error::AppError;
use crate::instances::mods::{InstalledMod, ModrinthKind, ModrinthProject, ModrinthProjectDetail};
use crate::instances::{
    Instance, InstanceManager, InstanceRunState, InstanceTab, LoaderKind, SortMode,
};
use crate::messages::Message;
use crate::state::{AppState, StartupData};
use crate::storage::{accounts, settings, SledStore};
use crate::theme::SwiftTheme;

pub struct SwiftLauncher {
    pub state: AppState,
    pub theme: SwiftTheme,
    store: Option<SledStore>,
    active_session: Option<Session>,
    accounts: Vec<Session>,
    instances: Vec<Instance>,
    settings: settings::LauncherSettings,
    search: String,
    sort: SortMode,
    list_view: bool,
    selected_instance: Option<String>,
    selected_tab: InstanceTab,
    loading_progress: f32,
    loading_status: String,
    login_provider: AuthProvider,
    username: String,
    password: String,
    totp: String,
    password_visible: bool,
    auth_busy: bool,
    microsoft_auth_id: u64,
    error_banner: Option<String>,
    device_flow: Option<(String, String)>,
    create_name: String,
    create_version: String,
    create_loader: LoaderKind,
    create_loader_version: String,
    create_versions: Vec<String>,
    create_loader_versions: Vec<String>,
    create_loader_versions_busy: bool,
    create_modal_open: bool,
    create_busy: bool,
    create_install_id: u64,
    create_install_status: String,
    create_install_progress: f32,
    create_install_control: Option<tokio::sync::watch::Sender<DownloadControl>>,
    create_install_paused: bool,
    import_modal_open: bool,
    import_path: String,
    import_busy: bool,
    export_path: String,
    export_busy: bool,
    java_status: String,
    mods_search: String,
    mod_import_path: String,
    modrinth_query: String,
    modrinth_kind: ModrinthKind,
    modrinth_results: Vec<ModrinthProject>,
    modrinth_detail: Option<ModrinthProjectDetail>,
    modrinth_markdown: Vec<markdown::Item>,
    modrinth_detail_busy: bool,
    modrinth_busy: bool,
    installed_mods: Vec<InstalledMod>,
    mods_loading: bool,
    modrinth_install_run_id: u64,
    active_modrinth_install: Option<ActiveModrinthInstall>,
    modrinth_install_status: String,
    modrinth_install_progress: f32,
    launch_log: Vec<String>,
    launch_run_id: u64,
    active_launches: Vec<ActiveLaunch>,
    account_menu_open: bool,
    settings_open: bool,
    delete_confirm_id: Option<String>,
    window_width: f32,
    avatar_cache: HashMap<String, PathBuf>,
    launch_status_by_instance: HashMap<String, String>,
    launch_progress_by_instance: HashMap<String, f32>,
    last_auto_scrolled_log_len: usize,
    adding_account: bool,
}

#[derive(Clone)]
struct ActiveLaunch {
    run_id: u64,
    instance: Instance,
    session: Session,
    stop_tx: tokio::sync::watch::Sender<bool>,
}

#[derive(Clone)]
struct ActiveModrinthInstall {
    run_id: u64,
    kind: ModrinthKind,
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
}

impl SwiftLauncher {
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            state: AppState::Loading,
            theme: SwiftTheme::default(),
            store: None,
            active_session: None,
            accounts: Vec::new(),
            instances: Vec::new(),
            settings: settings::LauncherSettings::default(),
            search: String::new(),
            sort: SortMode::Name,
            list_view: false,
            selected_instance: None,
            selected_tab: InstanceTab::Overview,
            loading_progress: 0.02,
            loading_status: "Opening storage...".into(),
            login_provider: AuthProvider::Microsoft,
            username: String::new(),
            password: String::new(),
            totp: String::new(),
            password_visible: false,
            auth_busy: false,
            microsoft_auth_id: 0,
            error_banner: None,
            device_flow: None,
            create_name: "New Instance".into(),
            create_version: "1.21.8".into(),
            create_loader: LoaderKind::Vanilla,
            create_loader_version: String::new(),
            create_versions: crate::instances::create::fallback_versions(),
            create_loader_versions: Vec::new(),
            create_loader_versions_busy: false,
            create_modal_open: false,
            create_busy: false,
            create_install_id: 0,
            create_install_status: "Choose a version to install".into(),
            create_install_progress: 0.0,
            create_install_control: None,
            create_install_paused: false,
            import_modal_open: false,
            import_path: String::new(),
            import_busy: false,
            export_path: String::new(),
            export_busy: false,
            java_status: "Java not checked".into(),
            mods_search: String::new(),
            mod_import_path: String::new(),
            modrinth_query: String::new(),
            modrinth_kind: ModrinthKind::Mods,
            modrinth_results: Vec::new(),
            modrinth_detail: None,
            modrinth_markdown: Vec::new(),
            modrinth_detail_busy: false,
            modrinth_busy: false,
            installed_mods: Vec::new(),
            mods_loading: false,
            modrinth_install_run_id: 0,
            active_modrinth_install: None,
            modrinth_install_status: String::new(),
            modrinth_install_progress: 0.0,
            launch_log: Vec::new(),
            launch_run_id: 0,
            active_launches: Vec::new(),
            account_menu_open: false,
            settings_open: false,
            delete_confirm_id: None,
            window_width: 1160.0,
            avatar_cache: HashMap::new(),
            launch_status_by_instance: HashMap::new(),
            launch_progress_by_instance: HashMap::new(),
            last_auto_scrolled_log_len: 0,
            adding_account: false,
        };
        (app, Task::perform(startup(), Message::StartupFinished))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartupFinished(result) => match result {
                Ok(data) => {
                    self.loading_progress = 1.0;
                    self.loading_status = "Ready".into();
                    self.active_session = data.session;
                    let accounts_for_avatars = data.accounts.clone();
                    self.accounts = data.accounts;
                    self.instances = data
                        .instances
                        .into_iter()
                        .map(|mut instance| {
                            if instance.artwork_path.is_none() {
                                instance.artwork_path =
                                    crate::instances::screenshots::refresh_artwork(&instance.path);
                            }
                            instance
                        })
                        .collect();
                    self.settings = data.settings.clone();
                    self.theme = SwiftTheme {
                        mode: self.settings.theme_mode,
                        accent: self.settings.accent,
                    };
                    self.store = SledStore::open().ok();
                    self.state = if self.active_session.is_some() {
                        AppState::Home
                    } else {
                        AppState::Login
                    };
                    let avatar_tasks: Vec<Task<Message>> = accounts_for_avatars
                        .into_iter()
                        .map(|account| {
                            let uuid = account.uuid.clone();
                            Task::perform(
                                async move {
                                    crate::auth::avatar::cache_avatar(&account)
                                        .await
                                        .ok()
                                        .flatten()
                                },
                                move |path| Message::AvatarCached {
                                    uuid: uuid.clone(),
                                    path,
                                },
                            )
                        })
                        .collect();
                    if avatar_tasks.is_empty() {
                        Task::none()
                    } else {
                        Task::batch(avatar_tasks)
                    }
                }
                Err(error) => {
                    self.state = AppState::Login;
                    self.error_banner = Some(error.to_string());
                    Task::none()
                }
            },
            Message::Tick(now) => {
                if matches!(self.state, AppState::Loading) {
                    let wave = (now.elapsed().as_millis() % 1000) as f32 / 1000.0;
                    self.loading_progress = (self.loading_progress + 0.01).max(wave).min(0.94);
                }
                if self.selected_tab == InstanceTab::Logs
                    && self.selected_instance.is_some()
                    && self.last_auto_scrolled_log_len != self.launch_log.len()
                {
                    self.last_auto_scrolled_log_len = self.launch_log.len();
                    return scrollable::snap_to(
                        ScrollableId::new("launch-log-scroll"),
                        RelativeOffset::END,
                    );
                }
                Task::none()
            }
            Message::WindowResized(width) => {
                self.window_width = width.max(420.0);
                Task::none()
            }
            Message::SearchChanged(value) => {
                self.search = value;
                Task::none()
            }
            Message::SortChanged(value) => {
                self.sort = value;
                Task::none()
            }
            Message::ToggleListView(value) => {
                self.list_view = value;
                Task::none()
            }
            Message::NewInstance => {
                self.create_modal_open = true;
                self.error_banner = None;
                if self.create_versions.len() <= crate::instances::create::fallback_versions().len()
                {
                    self.create_versions.clear();
                    self.create_version.clear();
                    Task::perform(
                        crate::instances::create::fetch_available_versions(),
                        Message::VersionsLoaded,
                    )
                } else {
                    Task::none()
                }
            }
            Message::ImportInstance => {
                self.import_modal_open = true;
                self.import_path.clear();
                self.error_banner = None;
                Task::none()
            }
            Message::PickImportZip => Task::perform(
                crate::system::pick_file("Import Swift instance", vec![("Zip", vec!["zip"])]),
                |path| {
                    Message::ImportPathChanged(
                        path.map(|path| path.display().to_string())
                            .unwrap_or_default(),
                    )
                },
            ),
            Message::ImportPathChanged(value) => {
                if !value.is_empty() {
                    self.import_path = value;
                }
                Task::none()
            }
            Message::ImportInstanceSubmit => {
                let path = std::path::PathBuf::from(self.import_path.trim());
                if path.as_os_str().is_empty() {
                    self.error_banner = Some("enter an instance zip path first".into());
                    return Task::none();
                }
                self.import_busy = true;
                Task::perform(
                    crate::instances::archive::import_instance(path),
                    Message::InstanceImported,
                )
            }
            Message::ImportInstanceCancel => {
                if self.import_busy {
                    return Task::none();
                }
                self.import_modal_open = false;
                self.import_path.clear();
                Task::none()
            }
            Message::InstanceImported(result) => {
                self.import_busy = false;
                match result {
                    Ok(instance) => {
                        let mut instance = instance;
                        apply_instance_defaults(&mut instance, &self.settings);
                        if let Some(store) = &self.store {
                            if let Err(error) = InstanceManager::new(store.clone()).save(&instance)
                            {
                                self.error_banner = Some(error.to_string());
                            }
                        }
                        self.instances.push(instance);
                        self.import_modal_open = false;
                        self.import_path.clear();
                    }
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::VersionsLoaded(result) => {
                match result {
                    Ok(versions) => {
                        if !versions.is_empty() {
                            if self.create_version.is_empty()
                                || !versions
                                    .iter()
                                    .any(|version| version == &self.create_version)
                            {
                                self.create_version = versions[0].clone();
                            }
                            self.create_versions = versions;
                        }
                    }
                    Err(error) => {
                        self.error_banner = Some(format!("Using cached version list: {error}"));
                    }
                }
                Task::none()
            }
            Message::LoaderVersionsLoaded(result) => {
                self.create_loader_versions_busy = false;
                match result {
                    Ok(versions) => {
                        self.create_loader_versions = versions;
                        self.create_loader_version = self
                            .create_loader_versions
                            .first()
                            .cloned()
                            .unwrap_or_default();
                    }
                    Err(error) => {
                        self.create_loader_versions.clear();
                        self.create_loader_version.clear();
                        self.error_banner = Some(error.to_string());
                    }
                }
                Task::none()
            }
            Message::CreateInstanceNameChanged(value) => {
                self.create_name = value;
                Task::none()
            }
            Message::CreateInstanceVersionChanged(value) => {
                self.create_version = value;
                self.refresh_loader_versions_task()
            }
            Message::CreateInstanceLoaderChanged(value) => {
                self.create_loader = value;
                self.refresh_loader_versions_task()
            }
            Message::CreateInstanceLoaderVersionChanged(value) => {
                self.create_loader_version = value;
                Task::none()
            }
            Message::CreateInstanceSubmit => {
                self.create_busy = true;
                self.create_install_paused = false;
                let (control_tx, _) = tokio::sync::watch::channel(DownloadControl::Run);
                self.create_install_control = Some(control_tx);
                self.create_install_id = self.create_install_id.wrapping_add(1);
                self.create_install_status =
                    format!("Starting Minecraft {} install", self.create_version);
                self.create_install_progress = 0.01;
                Task::none()
            }
            Message::CreateInstanceCancel => {
                if self.create_busy {
                    self.create_install_status = "Install is running; cancel support comes with download manager pause/cancel".into();
                    return Task::none();
                }
                self.create_modal_open = false;
                self.create_busy = false;
                self.create_install_status = "Choose a version to install".into();
                self.create_install_progress = 0.0;
                self.create_loader_versions_busy = false;
                self.create_install_control = None;
                self.create_install_paused = false;
                Task::none()
            }
            Message::CreateInstallPause => {
                if let Some(tx) = &self.create_install_control {
                    let _ = tx.send(DownloadControl::Pause);
                    self.create_install_paused = true;
                    self.create_install_status = format!("Paused: {}", self.create_install_status);
                }
                Task::none()
            }
            Message::CreateInstallResume => {
                if let Some(tx) = &self.create_install_control {
                    let _ = tx.send(DownloadControl::Run);
                    self.create_install_paused = false;
                    if let Some(status) = self.create_install_status.strip_prefix("Paused: ") {
                        self.create_install_status = status.to_string();
                    }
                }
                Task::none()
            }
            Message::CreateInstallCancel => {
                if let Some(tx) = &self.create_install_control {
                    let _ = tx.send(DownloadControl::Cancel);
                    self.create_install_status = "Cancelling install...".into();
                }
                Task::none()
            }
            Message::InstallStatusChanged(status) => {
                self.create_install_status = status;
                Task::none()
            }
            Message::InstallProgressChanged { status, progress } => {
                self.create_install_status = status;
                self.create_install_progress = progress;
                Task::none()
            }
            Message::InstanceCreated(result) => {
                self.create_busy = false;
                self.create_install_control = None;
                self.create_install_paused = false;
                match result {
                    Ok(instance) => {
                        let mut instance = instance;
                        apply_instance_defaults(&mut instance, &self.settings);
                        if let Some(store) = &self.store {
                            if let Err(error) = InstanceManager::new(store.clone()).save(&instance)
                            {
                                self.error_banner = Some(error.to_string());
                            }
                        }
                        self.instances.push(instance);
                        self.create_modal_open = false;
                        self.create_install_status = "Install complete".into();
                        self.create_install_progress = 1.0;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        self.create_install_status = format!("Install failed: {message}");
                        self.error_banner = Some(message);
                    }
                }
                Task::none()
            }
            Message::SelectInstance(id) => {
                self.selected_instance = Some(id);
                self.selected_tab = InstanceTab::Overview;
                Task::none()
            }
            Message::OpenInstanceTab(id, tab) => {
                self.selected_instance = Some(id);
                self.selected_tab = tab;
                if tab == InstanceTab::Mods {
                    self.load_selected_mods()
                } else {
                    Task::none()
                }
            }
            Message::CloseInstanceDetail => {
                self.selected_instance = None;
                Task::none()
            }
            Message::SelectInstanceTab(tab) => {
                self.selected_tab = tab;
                if tab == InstanceTab::Mods {
                    self.load_selected_mods()
                } else {
                    Task::none()
                }
            }
            Message::InstanceNameChanged(value) => {
                self.with_selected_instance(|instance| instance.name = value);
                self.persist_selected();
                Task::none()
            }
            Message::RamChanged(value) => {
                self.with_selected_instance(|instance| instance.ram_mb = value);
                self.persist_selected();
                Task::none()
            }
            Message::JavaPathChanged(value) => {
                self.with_selected_instance(|instance| instance.java_path = value);
                self.persist_selected();
                Task::none()
            }
            Message::JvmArgsChanged(value) => {
                self.with_selected_instance(|instance| instance.jvm_args = value);
                self.persist_selected();
                Task::none()
            }
            Message::ResolutionWidthChanged(value) => {
                if let Ok(width) = value.parse() {
                    self.with_selected_instance(|instance| instance.resolution_width = width);
                    self.persist_selected();
                }
                Task::none()
            }
            Message::ResolutionHeightChanged(value) => {
                if let Ok(height) = value.parse() {
                    self.with_selected_instance(|instance| instance.resolution_height = height);
                    self.persist_selected();
                }
                Task::none()
            }
            Message::FullscreenChanged(value) => {
                self.with_selected_instance(|instance| instance.fullscreen = value);
                self.persist_selected();
                Task::none()
            }
            Message::GameDirOverrideChanged(value) => {
                self.with_selected_instance(|instance| instance.game_dir_override = value);
                self.persist_selected();
                Task::none()
            }
            Message::ServerChanged(value) => {
                self.with_selected_instance(|instance| instance.server = value);
                self.persist_selected();
                Task::none()
            }
            Message::PlayInstance(id) => {
                let instance = self
                    .instances
                    .iter()
                    .find(|instance| instance.id == id)
                    .cloned();
                let session = self.active_session.clone();
                if let Some(target) = self.instances.iter_mut().find(|instance| instance.id == id) {
                    target.run_state = InstanceRunState::Preparing;
                }
                self.launch_progress_by_instance.insert(id.clone(), 0.0);
                match (instance, session) {
                    (Some(instance), Some(session)) => {
                        let (stop_tx, _) = tokio::sync::watch::channel(false);
                        self.launch_run_id = self.launch_run_id.wrapping_add(1);
                        self.launch_log
                            .push(format!("preparing launch: {}", instance.name));
                        self.active_launches.push(ActiveLaunch {
                            run_id: self.launch_run_id,
                            instance,
                            session,
                            stop_tx,
                        });
                        Task::none()
                    }
                    (None, _) => {
                        self.error_banner = Some("instance missing".into());
                        Task::none()
                    }
                    (_, None) => {
                        self.error_banner = Some("login required before launch".into());
                        Task::none()
                    }
                }
            }
            Message::StopInstance(id) => {
                for launch in self
                    .active_launches
                    .iter()
                    .filter(|launch| launch.instance.id == id)
                {
                    let _ = launch.stop_tx.send(true);
                }
                if let Some(instance) = self.instances.iter_mut().find(|instance| instance.id == id)
                {
                    instance.run_state = InstanceRunState::Preparing;
                }
                Task::none()
            }
            Message::LaunchStarted { instance_id, pid } => {
                if let Some(name) = self
                    .instances
                    .iter_mut()
                    .find(|instance| instance.id == instance_id)
                    .map(|instance| {
                        instance.run_state = InstanceRunState::Preparing;
                        instance.name.clone()
                    })
                {
                    self.launch_status_by_instance
                        .insert(instance_id.clone(), "Starting process...".into());
                    self.launch_progress_by_instance.insert(instance_id, 1.0);
                    self.push_launch_log(format!("{name} process started (pid {pid})"));
                }
                Task::none()
            }
            Message::LaunchReady { instance_id } => {
                if let Some(name) = self
                    .instances
                    .iter_mut()
                    .find(|instance| instance.id == instance_id)
                    .map(|instance| {
                        instance.run_state = InstanceRunState::Running;
                        instance.name.clone()
                    })
                {
                    self.launch_status_by_instance
                        .insert(instance_id.clone(), "In game".into());
                    self.launch_progress_by_instance.remove(&instance_id);
                    self.push_launch_log(format!("{name} is running"));
                }
                Task::none()
            }
            Message::LaunchPrepareProgress {
                instance_id,
                status,
                progress,
            } => {
                let label = format!("{status} ({:.0}%)", progress * 100.0);
                self.launch_progress_by_instance
                    .insert(instance_id.clone(), progress.clamp(0.0, 1.0));
                self.launch_status_by_instance.insert(instance_id, label);
                Task::none()
            }
            Message::LaunchFinished(result) => {
                for instance in &mut self.instances {
                    if matches!(instance.run_state, InstanceRunState::Preparing) {
                        instance.run_state = if result.is_ok() {
                            InstanceRunState::Running
                        } else {
                            InstanceRunState::Idle
                        };
                    }
                }
                match result {
                    Ok(line) => self.launch_log.push(line),
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::LaunchOutput { instance_id, line } => {
                let name = self
                    .instances
                    .iter()
                    .find(|instance| instance.id == instance_id)
                    .map(|instance| instance.name.as_str())
                    .unwrap_or("launch");
                self.push_launch_log(format!("[{name}] {line}"));
                Task::none()
            }
            Message::LaunchExited {
                instance_id,
                status,
                success,
                game_ready,
                runtime_seconds,
                playtime_seconds,
                crash_report,
            } => {
                self.active_launches
                    .retain(|launch| launch.instance.id != instance_id);
                self.launch_status_by_instance.remove(&instance_id);
                self.launch_progress_by_instance.remove(&instance_id);
                let exit_log;
                let error_msg;
                if let Some(instance) = self
                    .instances
                    .iter_mut()
                    .find(|instance| instance.id == instance_id)
                {
                    instance.run_state = InstanceRunState::Idle;
                    if game_ready {
                        instance.playtime_seconds =
                            instance.playtime_seconds.saturating_add(playtime_seconds);
                        instance.last_played_unix = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .ok()
                            .map(|duration| duration.as_secs());
                    }
                    if let Some(shot) =
                        crate::instances::screenshots::refresh_artwork(&instance.path)
                    {
                        instance.artwork_path = Some(shot);
                    }
                    let name = instance.name.clone();
                    let verb = if success {
                        "exited"
                    } else if game_ready {
                        "crashed"
                    } else {
                        "failed to launch"
                    };
                    exit_log = format!("{name} {verb}: {status} ({runtime_seconds}s total)");
                    error_msg = if !success {
                        let tail = self
                            .launch_log
                            .iter()
                            .rev()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join(" | ");
                        Some(if tail.is_empty() {
                            format!("{name} crashed: {status}")
                        } else {
                            format!("{name} crashed: {status}. Last log: {tail}")
                        })
                    } else {
                        None
                    };
                } else {
                    exit_log = format!("launch ended: {status}");
                    error_msg = None;
                }
                self.push_launch_log(exit_log);
                if let Some(message) = error_msg {
                    self.error_banner = Some(message);
                }
                if let Some(store) = &self.store {
                    if let Some(instance) = self
                        .instances
                        .iter()
                        .find(|instance| instance.id == instance_id)
                    {
                        let _ = InstanceManager::new(store.clone()).save(instance);
                    }
                }
                if let Some(result) = crash_report {
                    match result {
                        Ok(path) => self.launch_log.push(format!("crash report saved: {path}")),
                        Err(error) => {
                            self.error_banner = Some(format!("crash report failed: {error}"))
                        }
                    }
                }
                Task::none()
            }
            Message::LaunchFailed { instance_id, error } => {
                self.active_launches
                    .retain(|launch| launch.instance.id != instance_id);
                if let Some(instance) = self
                    .instances
                    .iter_mut()
                    .find(|instance| instance.id == instance_id)
                {
                    instance.run_state = InstanceRunState::Idle;
                }
                self.launch_status_by_instance.remove(&instance_id);
                self.launch_progress_by_instance.remove(&instance_id);
                self.error_banner = Some(error.to_string());
                Task::none()
            }
            Message::LaunchLog(line) => {
                self.launch_log.push(line);
                Task::none()
            }
            Message::AssetsVerified { current, total } => {
                self.launch_log
                    .push(format!("assets verified {current}/{total}"));
                Task::none()
            }
            Message::OpenInstanceFiles(id) => self.open_instance_path_task(&id, ""),
            Message::OpenInstanceLogs(id) => self.open_instance_path_task(&id, "logs"),
            Message::OpenInstanceCrashReports(id) => {
                self.open_instance_path_task(&id, "crash-reports")
            }
            Message::OpenInstanceScreenshots(id) => {
                self.open_instance_path_task(&id, "screenshots")
            }
            Message::OpenInstanceResourcePacks(id) => {
                self.open_instance_path_task(&id, "resourcepacks")
            }
            Message::PathOpened(result) => {
                match result {
                    Ok(line) => self.launch_log.push(line),
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::ExportPathChanged(value) => {
                if !value.is_empty() {
                    self.export_path = value;
                }
                Task::none()
            }
            Message::PickExportZip(id) => {
                let name = self
                    .instances
                    .iter()
                    .find(|instance| instance.id == id)
                    .map(|instance| format!("{}.zip", instance.name.replace(' ', "-")))
                    .unwrap_or_else(|| "swift-instance.zip".into());
                Task::perform(
                    crate::system::save_file(
                        "Export Swift instance",
                        name,
                        vec![("Zip", vec!["zip"])],
                    ),
                    |path| {
                        Message::ExportPathChanged(
                            path.map(|path| path.display().to_string())
                                .unwrap_or_default(),
                        )
                    },
                )
            }
            Message::ExportInstance(id) => {
                let Some(instance) = self
                    .instances
                    .iter()
                    .find(|instance| instance.id == id)
                    .cloned()
                else {
                    self.error_banner = Some("instance missing".into());
                    return Task::none();
                };
                let path = std::path::PathBuf::from(self.export_path.trim());
                if path.as_os_str().is_empty() {
                    self.error_banner = Some("enter an export zip path first".into());
                    return Task::none();
                }
                self.export_busy = true;
                Task::perform(
                    crate::instances::archive::export_instance(instance, path),
                    Message::InstanceExported,
                )
            }
            Message::InstanceExported(result) => {
                self.export_busy = false;
                match result {
                    Ok(path) => self.launch_log.push(format!("instance exported: {path}")),
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::RequestDeleteInstance(id) => {
                self.delete_confirm_id = Some(id);
                Task::none()
            }
            Message::CancelDeleteInstance => {
                self.delete_confirm_id = None;
                Task::none()
            }
            Message::DeleteInstance(id) => {
                self.remove_instance_metadata(&id);
                Task::none()
            }
            Message::DeleteInstanceWithFiles(id) => {
                let instance = self
                    .instances
                    .iter()
                    .find(|instance| instance.id == id)
                    .cloned();
                self.remove_instance_metadata(&id);
                match instance {
                    Some(instance) => Task::perform(
                        crate::instances::delete_instance_files(instance),
                        Message::InstanceFilesDeleted,
                    ),
                    None => {
                        self.error_banner = Some("instance missing".into());
                        Task::none()
                    }
                }
            }
            Message::InstanceFilesDeleted(result) => {
                match result {
                    Ok(line) => self.launch_log.push(line),
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::ModsLoaded(result) => {
                self.mods_loading = false;
                match result {
                    Ok(mods) => self.installed_mods = mods,
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::ModsSearchChanged(value) => {
                self.mods_search = value;
                Task::none()
            }
            Message::ToggleMod { mod_id, enabled } => {
                let Some(path) = self.selected_instance_path() else {
                    self.error_banner = Some("select an instance first".into());
                    return Task::none();
                };
                self.mods_loading = true;
                Task::perform(
                    crate::instances::mods::set_mod_enabled(path, mod_id, enabled),
                    Message::ModToggled,
                )
            }
            Message::ModToggled(result) => {
                self.mods_loading = false;
                match result {
                    Ok(mods) => self.installed_mods = mods,
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::DeleteMod(mod_id) => {
                let Some(path) = self.selected_instance_path() else {
                    self.error_banner = Some("select an instance first".into());
                    return Task::none();
                };
                self.mods_loading = true;
                Task::perform(
                    crate::instances::mods::delete_mod(path, mod_id),
                    Message::ModDeleted,
                )
            }
            Message::ModDeleted(result) => {
                self.mods_loading = false;
                match result {
                    Ok(mods) => self.installed_mods = mods,
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::AddMod => {
                self.mod_import_path.clear();
                self.launch_log.push("add mod path input opened".into());
                Task::none()
            }
            Message::PickModJar => Task::perform(
                crate::system::pick_file(
                    "Import Minecraft mod",
                    vec![("Java archive", vec!["jar"])],
                ),
                |path| {
                    Message::ModImportPathChanged(
                        path.map(|path| path.display().to_string())
                            .unwrap_or_default(),
                    )
                },
            ),
            Message::ModImportPathChanged(value) => {
                if !value.is_empty() {
                    self.mod_import_path = value;
                }
                Task::none()
            }
            Message::ImportModSubmit => {
                let Some(instance_path) = self.selected_instance_path() else {
                    self.error_banner = Some("select an instance first".into());
                    return Task::none();
                };
                let source = std::path::PathBuf::from(self.mod_import_path.trim());
                if source.as_os_str().is_empty() {
                    self.error_banner = Some("enter a mod .jar path first".into());
                    return Task::none();
                }
                self.mods_loading = true;
                Task::perform(
                    crate::instances::mods::import_mod(instance_path, source),
                    Message::ModImported,
                )
            }
            Message::ModImported(result) => {
                self.mods_loading = false;
                match result {
                    Ok(mods) => {
                        self.installed_mods = mods;
                        self.mod_import_path.clear();
                    }
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::ModrinthSearchChanged(value) => {
                self.modrinth_query = value;
                Task::none()
            }
            Message::ModrinthKindSelected(kind) => {
                self.modrinth_kind = kind;
                self.modrinth_results.clear();
                self.modrinth_detail = None;
                self.modrinth_markdown.clear();
                Task::none()
            }
            Message::SearchModrinth => {
                let Some(instance) = self.selected_instance().cloned() else {
                    self.error_banner = Some("select an instance first".into());
                    return Task::none();
                };
                self.modrinth_busy = true;
                Task::perform(
                    crate::instances::mods::search_modrinth(
                        self.modrinth_query.clone(),
                        instance.minecraft_version,
                        instance.loader,
                        self.modrinth_kind,
                    ),
                    Message::ModrinthSearchFinished,
                )
            }
            Message::ModrinthSearchFinished(result) => {
                self.modrinth_busy = false;
                match result {
                    Ok(results) => self.modrinth_results = results,
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::OpenModrinthProject(project_id) => {
                self.modrinth_detail = None;
                self.modrinth_markdown.clear();
                self.modrinth_detail_busy = true;
                let kind = self.modrinth_kind;
                Task::perform(
                    crate::instances::mods::modrinth_project_detail(project_id, kind),
                    Message::ModrinthProjectDetailLoaded,
                )
            }
            Message::ModrinthProjectDetailLoaded(result) => {
                self.modrinth_detail_busy = false;
                match result {
                    Ok(detail) => {
                        self.modrinth_markdown = markdown::parse(&detail.body).collect();
                        self.modrinth_detail = Some(detail);
                    }
                    Err(error) => self.error_banner = Some(error.to_string()),
                }
                Task::none()
            }
            Message::CloseModrinthProject => {
                self.modrinth_detail = None;
                self.modrinth_markdown.clear();
                Task::none()
            }
            Message::InstallModrinthProject(project_id) => {
                let Some(instance) = self.selected_instance().cloned() else {
                    self.error_banner = Some("select an instance first".into());
                    return Task::none();
                };
                self.modrinth_install_run_id = self.modrinth_install_run_id.wrapping_add(1);
                self.active_modrinth_install = Some(ActiveModrinthInstall {
                    run_id: self.modrinth_install_run_id,
                    kind: self.modrinth_kind,
                    instance_path: instance.path,
                    minecraft_version: instance.minecraft_version,
                    loader: instance.loader,
                    project_id,
                });
                self.modrinth_install_status = format!("Starting {} install", self.modrinth_kind);
                self.modrinth_install_progress = 0.01;
                self.mods_loading = true;
                Task::none()
            }
            Message::ModrinthProjectInstallProgress { status, progress } => {
                self.modrinth_install_status = status;
                self.modrinth_install_progress = progress;
                Task::none()
            }
            Message::ModrinthProjectInstalled(result) => {
                self.mods_loading = false;
                self.active_modrinth_install = None;
                match result {
                    Ok(mods) => {
                        self.installed_mods = mods;
                        self.modrinth_install_status = "Install complete".into();
                        self.modrinth_install_progress = 1.0;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        self.modrinth_install_status = format!("Install failed: {message}");
                        self.error_banner = Some(message);
                    }
                }
                Task::none()
            }
            Message::AuthProviderSelected(provider) => {
                self.login_provider = provider;
                self.error_banner = None;
                Task::none()
            }
            Message::UsernameChanged(value) => {
                self.username = value;
                Task::none()
            }
            Message::PasswordChanged(value) => {
                self.password = value;
                Task::none()
            }
            Message::TotpChanged(value) => {
                self.totp = value;
                Task::none()
            }
            Message::TogglePasswordVisible => {
                self.password_visible = !self.password_visible;
                Task::none()
            }
            Message::SubmitLogin => {
                if self.auth_busy {
                    return Task::none();
                }
                self.auth_busy = true;
                self.error_banner = None;
                if self.login_provider == AuthProvider::Microsoft {
                    self.device_flow = None;
                    self.microsoft_auth_id = self.microsoft_auth_id.wrapping_add(1);
                    Task::none()
                } else {
                    let mut password = self.password.clone();
                    if self.login_provider == AuthProvider::ElyBy && !self.totp.trim().is_empty() {
                        password = format!("{}:{}", password, self.totp.trim());
                    }
                    Task::perform(
                        crate::auth::authenticate(
                            self.login_provider,
                            self.username.clone(),
                            password,
                        ),
                        Message::AuthFinished,
                    )
                }
            }
            Message::MicrosoftDeviceReady {
                user_code,
                verification_uri,
            } => {
                self.device_flow = Some((user_code, verification_uri));
                Task::none()
            }
            Message::MicrosoftApproved(result) | Message::AuthFinished(result) => {
                self.auth_busy = false;
                match result {
                    Ok(session) => {
                        self.accept_session(session.clone());
                        Self::cache_avatar_task(session)
                    }
                    Err(error) => {
                        let message = error.to_string();
                        if self.login_provider == AuthProvider::ElyBy
                            && message.contains("two factor auth")
                        {
                            self.error_banner = Some("Ely.by account has 2FA enabled. Enter current 2FA code, then sign in again.".into());
                        } else {
                            self.error_banner = Some(message);
                        }
                        Task::none()
                    }
                }
            }
            Message::CopyVerificationUrl => {
                if let Some((_, url)) = &self.device_flow {
                    let url = url.clone();
                    self.launch_log.push("verification URL copied".into());
                    Task::batch([iced::clipboard::write(url)])
                } else {
                    Task::none()
                }
            }
            Message::CopyLogs => {
                let text = self.launch_log.join("\n");
                Task::batch([iced::clipboard::write(text)])
            }
            Message::AccountMenuToggled => {
                self.account_menu_open = !self.account_menu_open;
                Task::none()
            }
            Message::AccountSelected(uuid) => {
                self.active_session = self
                    .accounts
                    .iter()
                    .find(|session| session.uuid == uuid)
                    .cloned();
                if let (Some(store), Some(session)) = (&self.store, &self.active_session) {
                    let _ = accounts::save_session(store, session);
                }
                self.account_menu_open = false;
                Task::none()
            }
            Message::SignOut(uuid) => {
                let session_to_invalidate = self
                    .accounts
                    .iter()
                    .find(|session| session.uuid == uuid)
                    .cloned();
                if let Some(store) = &self.store {
                    if let Some(session) = self.accounts.iter().find(|session| session.uuid == uuid)
                    {
                        let _ = accounts::remove_session(store, session);
                    }
                }
                self.accounts.retain(|session| session.uuid != uuid);
                if self
                    .active_session
                    .as_ref()
                    .map(|session| session.uuid.as_str())
                    == Some(uuid.as_str())
                {
                    self.active_session = None;
                    self.state = AppState::Login;
                }
                match session_to_invalidate {
                    Some(session) => Task::perform(
                        async move { crate::auth::invalidate(&session).await },
                        |_| Message::Noop,
                    ),
                    None => Task::none(),
                }
            }
            Message::AddAccount => {
                self.settings_open = false;
                self.account_menu_open = false;
                self.adding_account = true;
                self.login_provider = AuthProvider::Microsoft;
                self.username.clear();
                self.password.clear();
                self.totp.clear();
                self.device_flow = None;
                self.error_banner = None;
                self.state = AppState::Login;
                Task::none()
            }
            Message::CancelAddAccount => {
                self.adding_account = false;
                self.auth_busy = false;
                self.device_flow = None;
                self.error_banner = None;
                if self.active_session.is_some() {
                    self.state = AppState::Home;
                    self.settings_open = true;
                }
                Task::none()
            }
            Message::SettingsOpened => {
                self.settings_open = true;
                Task::none()
            }
            Message::SettingsClosed => {
                self.settings_open = false;
                Task::none()
            }
            Message::ThemeModeChanged(value) => {
                self.settings.theme_mode = value;
                self.theme.mode = value;
                self.persist_settings();
                Task::none()
            }
            Message::AccentChanged(value) => {
                self.settings.accent = value;
                self.theme.accent = value;
                self.persist_settings();
                Task::none()
            }
            Message::UiScaleChanged(value) => {
                self.settings.ui_scale = value;
                self.persist_settings();
                Task::none()
            }
            Message::DefaultJavaChanged(value) => {
                self.settings.default_java_path = value;
                self.java_status = "Java not checked".into();
                self.persist_settings();
                Task::none()
            }
            Message::PickDefaultJava => Task::perform(
                crate::system::pick_file(
                    "Choose Java executable",
                    vec![("Java", vec!["java", "exe"])],
                ),
                |path| {
                    Message::DefaultJavaChanged(
                        path.map(|path| path.display().to_string())
                            .unwrap_or_default(),
                    )
                },
            ),
            Message::ValidateDefaultJava => {
                let java = self.settings.default_java_path.clone();
                self.java_status = "Checking Java...".into();
                Task::perform(
                    async move {
                        crate::download::java::detect_java(java).await.map(|info| {
                            format!("Detected Java {} at {}", info.major, info.path.display())
                        })
                    },
                    Message::DefaultJavaValidated,
                )
            }
            Message::DefaultJavaValidated(result) => {
                match result {
                    Ok(status) => self.java_status = status,
                    Err(error) => {
                        self.java_status = "Java check failed".into();
                        self.error_banner = Some(error.to_string());
                    }
                }
                Task::none()
            }
            Message::DownloadManagedJava(version) => {
                self.java_status = format!("Downloading managed Java {version}...");
                Task::perform(
                    async move {
                        crate::download::java::download_managed_java(version)
                            .await
                            .map(|info| {
                                format!(
                                    "Managed Java {} ready: {}",
                                    info.major,
                                    info.path.display()
                                )
                            })
                    },
                    Message::ManagedJavaReady,
                )
            }
            Message::ManagedJavaReady(result) => {
                match result {
                    Ok(status) => self.java_status = status,
                    Err(error) => {
                        self.java_status = "Managed Java download failed".into();
                        self.error_banner = Some(error.to_string());
                    }
                }
                Task::none()
            }
            Message::OpenManagedJavaDir => match crate::download::java::managed_java_root() {
                Ok(path) => Task::perform(crate::system::open_path(path), Message::PathOpened),
                Err(error) => {
                    self.error_banner = Some(error.to_string());
                    Task::none()
                }
            },
            Message::DefaultRamChanged(value) => {
                self.settings.default_ram_mb = value;
                self.persist_settings();
                Task::none()
            }
            Message::GlobalJvmArgsChanged(value) => {
                self.settings.global_jvm_args = value;
                self.persist_settings();
                Task::none()
            }
            Message::DefaultGameDirChanged(value) => {
                self.settings.default_game_dir = value.into();
                self.persist_settings();
                Task::none()
            }
            Message::DiscordPresenceChanged(value) => {
                self.settings.discord_presence = value;
                self.persist_settings();
                Task::none()
            }
            Message::CrashReporterChanged(value) => {
                self.settings.crash_reporter = value;
                self.persist_settings();
                Task::none()
            }
            Message::OpenExternal(url) => {
                Task::perform(crate::system::open_url(url), Message::PathOpened)
            }
            Message::DownloadProgress(progress) => {
                self.launch_log.push(format!(
                    "download {} {}",
                    progress.job_id, progress.downloaded_bytes
                ));
                Task::none()
            }
            Message::DownloadEvent(event) => {
                self.launch_log.push(format!("download event: {event:?}"));
                Task::none()
            }
            Message::ErrorDismissed => {
                self.error_banner = None;
                Task::none()
            }
            Message::AvatarCached { uuid, path } => {
                if let Some(path) = path {
                    self.avatar_cache.insert(uuid, path);
                }
                Task::none()
            }
            Message::Noop | Message::FilePicked(_) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base = match self.state {
            AppState::Loading => {
                crate::screens::loading::view(self.loading_progress, &self.loading_status)
            }
            AppState::Login => crate::screens::login::view(
                self.login_provider,
                &self.username,
                &self.password,
                &self.totp,
                self.password_visible,
                self.auth_busy,
                self.error_banner.as_deref(),
                self.device_flow
                    .as_ref()
                    .map(|(code, url)| (code.as_str(), url.as_str())),
                self.adding_account,
            ),
            AppState::Home => crate::screens::home::view(
                self.active_session.as_ref(),
                &self.instances,
                &self.avatar_cache,
                self.window_width,
                &self.search,
                self.sort,
                self.list_view,
                self.settings_open,
                &self.settings,
                &self.java_status,
                self.create_modal_open,
                self.import_modal_open,
                &self.create_versions,
                &self.create_loader_versions,
                self.create_loader_versions_busy,
                self.create_busy,
                self.import_busy,
                &self.create_name,
                &self.create_version,
                self.create_loader,
                &self.create_loader_version,
                &self.import_path,
                &self.create_install_status,
                self.create_install_progress,
                self.create_install_paused,
                &self.accounts,
                self.account_menu_open,
                self.error_banner.as_deref(),
                self.delete_confirm_id.as_deref(),
            ),
        };

        if let Some(id) = &self.selected_instance {
            if let Some(instance) = self.instances.iter().find(|instance| &instance.id == id) {
                let launch_status = self
                    .launch_status_by_instance
                    .get(id)
                    .map(String::as_str)
                    .or_else(|| match instance.run_state {
                        crate::instances::InstanceRunState::Preparing => Some("Launching..."),
                        crate::instances::InstanceRunState::Running => Some("Running"),
                        crate::instances::InstanceRunState::Idle => None,
                    });
                let launch_progress = self.launch_progress_by_instance.get(id).copied();
                let detail = crate::screens::instance_detail::view(
                    instance,
                    self.selected_tab,
                    &self.mods_search,
                    &self.mod_import_path,
                    &self.export_path,
                    self.export_busy,
                    &self.modrinth_query,
                    self.modrinth_kind,
                    &self.modrinth_results,
                    self.modrinth_detail.as_ref(),
                    &self.modrinth_markdown,
                    self.modrinth_detail_busy,
                    self.modrinth_busy,
                    &self.installed_mods,
                    self.mods_loading,
                    &self.modrinth_install_status,
                    self.modrinth_install_progress,
                    &self.launch_log,
                    launch_status,
                    launch_progress,
                );
                return iced::widget::stack![base, detail]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
        }
        base
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            time::every(Duration::from_millis(16)).map(Message::Tick),
            iced::window::resize_events().map(|(_id, size)| Message::WindowResized(size.width)),
        ];

        if self.create_busy {
            subscriptions.push(install_subscription(
                self.create_install_id,
                self.create_name.clone(),
                self.create_version.clone(),
                self.create_loader,
                if self.create_loader_version.is_empty() {
                    None
                } else {
                    Some(self.create_loader_version.clone())
                },
                self.settings.default_game_dir.clone(),
                self.create_install_control
                    .as_ref()
                    .map(tokio::sync::watch::Sender::subscribe),
            ));
        }

        if self.auth_busy && self.login_provider == AuthProvider::Microsoft {
            subscriptions.push(microsoft_auth_subscription(self.microsoft_auth_id));
        }

        for launch in &self.active_launches {
            subscriptions.push(launch_subscription(
                launch.run_id,
                launch.instance.clone(),
                launch.session.clone(),
                launch.stop_tx.subscribe(),
                self.settings.crash_reporter,
            ));
        }

        if let Some(install) = &self.active_modrinth_install {
            subscriptions.push(modrinth_install_subscription(install.clone()));
        }

        Subscription::batch(subscriptions)
    }

    pub fn scale_factor(&self) -> f64 {
        (self.settings.ui_scale.clamp(75, 150) as f64) / 100.0
    }

    fn accept_session(&mut self, session: Session) {
        if let Some(store) = &self.store {
            if let Err(error) = accounts::save_session(store, &session) {
                self.error_banner = Some(error.to_string());
            }
        }
        self.active_session = Some(session.clone());
        if !self.accounts.iter().any(|saved| saved.uuid == session.uuid) {
            self.accounts.push(session);
        }
        self.state = AppState::Home;
        self.adding_account = false;
        self.settings_open = false;
    }

    fn cache_avatar_task(session: Session) -> Task<Message> {
        let uuid = session.uuid.clone();
        Task::perform(
            async move {
                crate::auth::avatar::cache_avatar(&session)
                    .await
                    .ok()
                    .flatten()
            },
            move |path| Message::AvatarCached {
                uuid: uuid.clone(),
                path,
            },
        )
    }

    fn push_launch_log(&mut self, line: String) {
        const MAX_LOG_LINES: usize = 500;
        self.launch_log.push(line);
        if self.launch_log.len() > MAX_LOG_LINES {
            let overflow = self.launch_log.len() - MAX_LOG_LINES;
            self.launch_log.drain(0..overflow);
        }
    }

    fn with_selected_instance(&mut self, f: impl FnOnce(&mut Instance)) {
        if let Some(id) = &self.selected_instance {
            if let Some(instance) = self
                .instances
                .iter_mut()
                .find(|instance| &instance.id == id)
            {
                f(instance);
            }
        }
    }

    fn persist_selected(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let Some(id) = &self.selected_instance else {
            return;
        };
        if let Some(instance) = self.instances.iter().find(|instance| &instance.id == id) {
            let _ = InstanceManager::new(store.clone()).save(instance);
        }
    }

    fn persist_settings(&self) {
        if let Some(store) = &self.store {
            let _ = settings::save(store, &self.settings);
        }
    }

    fn selected_instance_path(&self) -> Option<std::path::PathBuf> {
        self.selected_instance()
            .map(|instance| instance.path.clone())
    }

    fn selected_instance(&self) -> Option<&Instance> {
        let id = self.selected_instance.as_ref()?;
        self.instances.iter().find(|instance| &instance.id == id)
    }

    fn load_selected_mods(&mut self) -> Task<Message> {
        let Some(path) = self.selected_instance_path() else {
            return Task::none();
        };
        self.mods_loading = true;
        Task::perform(
            async move { crate::instances::mods::list_mods(&path).await },
            Message::ModsLoaded,
        )
    }

    fn open_instance_path_task(&mut self, id: &str, child: &str) -> Task<Message> {
        let Some(instance) = self.instances.iter().find(|instance| instance.id == id) else {
            self.error_banner = Some("instance missing".into());
            return Task::none();
        };
        let path = if child.is_empty() {
            instance.path.clone()
        } else {
            instance.path.join(child)
        };
        Task::perform(crate::system::open_path(path), Message::PathOpened)
    }

    fn refresh_loader_versions_task(&mut self) -> Task<Message> {
        self.create_loader_versions.clear();
        self.create_loader_version.clear();
        if self.create_loader == LoaderKind::Vanilla {
            self.create_loader_versions_busy = false;
            return Task::none();
        }
        self.create_loader_versions_busy = true;
        let loader = self.create_loader;
        let minecraft_version = self.create_version.clone();
        Task::perform(
            async move {
                crate::instances::install::fetch_loader_versions(loader, &minecraft_version).await
            },
            Message::LoaderVersionsLoaded,
        )
    }

    fn remove_instance_metadata(&mut self, id: &str) {
        self.instances.retain(|instance| instance.id != id);
        if self.selected_instance.as_deref() == Some(id) {
            self.selected_instance = None;
        }
        self.delete_confirm_id = None;
        if let Some(store) = &self.store {
            if let Err(error) = InstanceManager::new(store.clone()).delete(id) {
                self.error_banner = Some(error.to_string());
            }
        }
    }
}

fn microsoft_auth_subscription(id: u64) -> Subscription<Message> {
    Subscription::run_with_id(
        ("microsoft-auth", id),
        stream::channel(20, move |mut output| async move {
            use iced::futures::SinkExt;

            let (device_tx, mut device_rx) = tokio::sync::mpsc::unbounded_channel();
            let auth = tokio::spawn(crate::auth::microsoft::authenticate_device(Some(device_tx)));
            tokio::pin!(auth);

            loop {
                tokio::select! {
                    Some((user_code, verification_uri)) = device_rx.recv() => {
                        let _ = output.send(Message::MicrosoftDeviceReady { user_code, verification_uri }).await;
                    }
                    result = &mut auth => {
                        let message = match result {
                            Ok(result) => Message::MicrosoftApproved(result),
                            Err(error) => Message::MicrosoftApproved(Err(AppError::Auth(error.to_string()))),
                        };
                        let _ = output.send(message).await;
                        break;
                    }
                }
            }
        }),
    )
}

fn install_subscription(
    id: u64,
    name: String,
    version: String,
    loader: LoaderKind,
    loader_version: Option<String>,
    root: PathBuf,
    control_rx: Option<tokio::sync::watch::Receiver<DownloadControl>>,
) -> Subscription<Message> {
    Subscription::run_with_id(
        ("install-instance", id),
        stream::channel(100, move |mut output| async move {
            use iced::futures::SinkExt;

            let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
            let install = tokio::spawn(
                crate::instances::create::create_instance_with_root_and_control(
                    name,
                    version,
                    loader,
                    loader_version,
                    root,
                    Some(status_tx),
                    control_rx,
                ),
            );

            tokio::pin!(install);

            loop {
                tokio::select! {
                    Some(progress) = status_rx.recv() => {
                        let _ = output.send(Message::InstallProgressChanged {
                            status: progress.status,
                            progress: progress.progress,
                        }).await;
                    }
                    result = &mut install => {
                        let message = match result {
                            Ok(result) => Message::InstanceCreated(result),
                            Err(error) => Message::InstanceCreated(Err(AppError::Instance(error.to_string()))),
                        };
                        let _ = output.send(message).await;
                        break;
                    }
                }
            }
        }),
    )
}

fn modrinth_install_subscription(install: ActiveModrinthInstall) -> Subscription<Message> {
    Subscription::run_with_id(
        ("modrinth-install", install.run_id),
        stream::channel(100, move |mut output| async move {
            use iced::futures::SinkExt;

            let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
            let task = tokio::spawn(
                crate::instances::mods::install_modrinth_project_with_status(
                    install.kind,
                    install.instance_path,
                    install.minecraft_version,
                    install.loader,
                    install.project_id,
                    Some(status_tx),
                ),
            );
            tokio::pin!(task);

            loop {
                tokio::select! {
                    Some(progress) = status_rx.recv() => {
                        let _ = output.send(Message::ModrinthProjectInstallProgress {
                            status: progress.status,
                            progress: progress.progress,
                        }).await;
                    }
                    result = &mut task => {
                        let message = match result {
                            Ok(result) => Message::ModrinthProjectInstalled(result),
                            Err(error) => Message::ModrinthProjectInstalled(Err(AppError::Instance(error.to_string()))),
                        };
                        let _ = output.send(message).await;
                        break;
                    }
                }
            }
        }),
    )
}

fn launch_subscription(
    id: u64,
    instance: Instance,
    session: Session,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    crash_reporter: bool,
) -> Subscription<Message> {
    Subscription::run_with_id(
        ("launch-instance", id),
        stream::channel(200, move |mut output| async move {
            use iced::futures::SinkExt;

            let instance_id = instance.id.clone();
            let report_instance = instance.clone();
            let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut prepare = tokio::spawn(
                crate::instances::launch::prepare_launch_command_with_status(
                    instance,
                    session,
                    Some(status_tx),
                ),
            );
            let mut command = loop {
                tokio::select! {
                    Some(progress) = status_rx.recv() => {
                        let _ = output.send(Message::LaunchPrepareProgress {
                            instance_id: instance_id.clone(),
                            status: progress.status,
                            progress: progress.progress,
                        }).await;
                    }
                    result = &mut prepare => {
                        match result {
                            Ok(Ok((command, java_line))) => {
                                let _ = output.send(Message::LaunchOutput {
                                    instance_id: instance_id.clone(),
                                    line: format!("java ready: {java_line}"),
                                }).await;
                                break command;
                            }
                            Ok(Err(error)) => {
                                let _ = output.send(Message::LaunchFailed {
                                    instance_id,
                                    error,
                                }).await;
                                return;
                            }
                            Err(error) => {
                                let _ = output.send(Message::LaunchFailed {
                                    instance_id,
                                    error: AppError::Process(format!("launch prep task failed: {error}")),
                                }).await;
                                return;
                            }
                        }
                    }
                }
            };

            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    let _ = output
                        .send(Message::LaunchFailed {
                            instance_id,
                            error: AppError::Process(error.to_string()),
                        })
                        .await;
                    return;
                }
            };

            let pid = child.id().unwrap_or_default();
            let mut monitor = crate::instances::launch_monitor::LaunchMonitor::new();
            let mut report_lines = Vec::new();
            let mut ready_sent = false;
            let _ = output
                .send(Message::LaunchStarted {
                    instance_id: instance_id.clone(),
                    pid,
                })
                .await;

            let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel();
            if let Some(stdout) = child.stdout.take() {
                tokio::spawn(forward_launch_lines(stdout, "stdout", line_tx.clone()));
            }
            if let Some(stderr) = child.stderr.take() {
                tokio::spawn(forward_launch_lines(stderr, "stderr", line_tx));
            }

            loop {
                tokio::select! {
                    Some(line) = line_rx.recv() => {
                        report_lines.push(line.clone());
                        if let Some(event) = monitor.on_line(&line) {
                            use crate::instances::launch_monitor::LaunchEvent;
                            if matches!(event, LaunchEvent::Ready) && !ready_sent {
                                ready_sent = true;
                                let _ = output.send(Message::LaunchReady {
                                    instance_id: instance_id.clone(),
                                }).await;
                            }
                        }
                        let _ = output.send(Message::LaunchOutput {
                            instance_id: instance_id.clone(),
                            line,
                        }).await;
                    }
                    changed = stop_rx.changed() => {
                        if changed.is_ok() && *stop_rx.borrow() {
                            let _ = output.send(Message::LaunchOutput {
                                instance_id: instance_id.clone(),
                                line: "stop requested".into(),
                            }).await;
                            let _ = child.kill().await;
                        }
                    }
                    result = child.wait() => {
                        let runtime_seconds = monitor.started_at.elapsed().as_secs();
                        let process_success = matches!(result, Ok(status) if status.success());
                        let process_status = match result {
                            Ok(status) => status.to_string(),
                            Err(error) => format!("wait failed: {error}"),
                        };
                        let outcome = monitor.finish(process_success, runtime_seconds);
                        let status = if outcome.summary.is_empty() {
                            process_status
                        } else {
                            format!("{} ({})", process_status, outcome.summary)
                        };
                        let should_report = crash_reporter
                            && (!outcome.success || outcome.crash_detected);
                        let crash_report = if should_report {
                            Some(crate::instances::crash::write_launch_crash_report(
                                &report_instance,
                                &status,
                                runtime_seconds,
                                &report_lines,
                            ).await)
                        } else {
                            None
                        };
                        let _ = output.send(Message::LaunchExited {
                            instance_id: instance_id.clone(),
                            status,
                            success: outcome.success,
                            game_ready: outcome.game_ready,
                            runtime_seconds,
                            playtime_seconds: outcome.playtime_seconds,
                            crash_report,
                        }).await;
                        break;
                    }
                }
            }
        }),
    )
}

async fn forward_launch_lines<R>(
    reader: R,
    label: &'static str,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let _ = tx.send(format!("{label}: {line}"));
            }
            Ok(None) => break,
            Err(error) => {
                let _ = tx.send(format!("{label} read error: {error}"));
                break;
            }
        }
    }
}

fn apply_instance_defaults(instance: &mut Instance, settings: &settings::LauncherSettings) {
    instance.java_path = settings.default_java_path.clone();
    instance.ram_mb = settings.default_ram_mb;
    if !settings.global_jvm_args.trim().is_empty() {
        instance.jvm_args = settings.global_jvm_args.clone();
    }
}

async fn startup() -> Result<StartupData, AppError> {
    let store = SledStore::open()?;
    let settings = settings::load(&store)?;
    let accounts = accounts::list_sessions(&store)?;
    let mut session = accounts::active_session(&store)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Storage(e.to_string()))?
        .as_secs();
    if let Some(stored) = session.clone() {
        let should_refresh = stored.expired_or_stale(now)
            || matches!(
                stored.provider,
                AuthProvider::ElyBy | AuthProvider::LittleSkin
            );
        if should_refresh {
            session = match crate::auth::refresh(&stored).await {
                Ok(refreshed) => {
                    accounts::save_session(&store, &refreshed)?;
                    Some(refreshed)
                }
                Err(_) => {
                    let _ = accounts::remove_session(&store, &stored);
                    None
                }
            };
        } else if crate::auth::validate(&stored).await.is_err() {
            let _ = accounts::remove_session(&store, &stored);
            session = None;
        }
    }
    let instances = InstanceManager::new(store).list()?;
    Ok(StartupData {
        session,
        accounts,
        instances,
        settings,
    })
}
