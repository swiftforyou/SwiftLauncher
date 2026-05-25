#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchEvent {
    Ready,
    CrashSignal,
}

#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    pub game_ready: bool,
    pub crash_detected: bool,
    pub success: bool,
    pub playtime_seconds: u64,
    pub summary: String,
}

#[derive(Debug)]
pub struct LaunchMonitor {
    game_ready: bool,
    crash_detected: bool,
    ready_at: Option<std::time::Instant>,
    pub started_at: std::time::Instant,
}

impl Default for LaunchMonitor {
    fn default() -> Self {
        Self {
            game_ready: false,
            crash_detected: false,
            ready_at: None,
            started_at: std::time::Instant::now(),
        }
    }
}

impl LaunchMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_line(&mut self, line: &str) -> Option<LaunchEvent> {
        let lower = line.to_ascii_lowercase();
        if !self.game_ready && is_ready_signal(line) {
            self.game_ready = true;
            self.ready_at = Some(std::time::Instant::now());
            return Some(LaunchEvent::Ready);
        }
        if is_crash_signal(&lower) {
            self.crash_detected = true;
            return Some(LaunchEvent::CrashSignal);
        }
        None
    }

    pub fn finish(&self, process_success: bool, runtime_seconds: u64) -> LaunchOutcome {
        let playtime_seconds = self
            .ready_at
            .map(|ready| ready.elapsed().as_secs().max(1))
            .unwrap_or(0);

        let failed_before_ready = !self.game_ready && !process_success && runtime_seconds < 180;
        let success = process_success && self.game_ready && !self.crash_detected && !failed_before_ready;

        let summary = if success {
            format!("exited after {playtime_seconds}s of play")
        } else if self.crash_detected {
            "crash detected in game logs".into()
        } else if failed_before_ready {
            "launch failed before the game became ready".into()
        } else if !process_success {
            format!("process exited with failure after {runtime_seconds}s")
        } else {
            format!("exited after {runtime_seconds}s")
        };

        LaunchOutcome {
            game_ready: self.game_ready,
            crash_detected: self.crash_detected,
            success,
            playtime_seconds,
            summary,
        }
    }
}

fn is_ready_signal(line: &str) -> bool {
    [
        "setting user:",
        "lwjgl version",
        "opengl version",
        "created:",
        "started on",
        "done initializing",
        "loading complete",
        "reload complete",
        "minecraftforge",
        "modlauncher running",
        "render thread",
        "game renderer",
        "connecting to",
        "joined the game",
        "singleplayer",
        "openjdk 64-bit",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

fn is_crash_signal(line: &str) -> bool {
    [
        "---- minecraft crash report ----",
        "game crashed",
        "fatal error",
        "a fatal error has occurred",
        "exception in thread",
        "could not find or load main class",
        "outofmemoryerror",
        "there was a severe problem",
        "process crashed with exit code",
        "crash report saved",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}
