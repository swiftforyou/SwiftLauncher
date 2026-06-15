use crate::auth::Session;
use crate::instances::Instance;

#[derive(Debug, Clone)]
pub enum AppState {
    Loading,
    Initialize,
    Login,
    Home,
}

#[derive(Debug, Clone)]
pub struct StartupData {
    pub session: Option<Session>,
    pub accounts: Vec<Session>,
    pub instances: Vec<Instance>,
    pub settings: crate::storage::settings::LauncherSettings,
}
