use crate::auth::Session;
use crate::error::AppError;
use crate::storage::{SledStore, KEY_ACCOUNT_PREFIX, KEY_ACTIVE_ACCOUNT};

pub fn account_key(session: &Session) -> String {
    format!(
        "{KEY_ACCOUNT_PREFIX}{}:{}",
        session.provider.as_key(),
        session.uuid
    )
}

pub fn save_session(store: &SledStore, session: &Session) -> Result<(), AppError> {
    store.set(&account_key(session), session)?;
    store.set(KEY_ACTIVE_ACCOUNT, &session.uuid)?;
    Ok(())
}

pub fn list_sessions(store: &SledStore) -> Result<Vec<Session>, AppError> {
    let mut sessions =
        store.scan_prefix_excluding::<Session>(KEY_ACCOUNT_PREFIX, &[KEY_ACTIVE_ACCOUNT])?;
    sessions.sort_by(|a, b| a.username.cmp(&b.username));
    Ok(sessions)
}

pub fn active_session(store: &SledStore) -> Result<Option<Session>, AppError> {
    let active_uuid = store.get::<String>(KEY_ACTIVE_ACCOUNT)?;
    let sessions = list_sessions(store)?;
    Ok(active_uuid.and_then(|uuid| sessions.into_iter().find(|session| session.uuid == uuid)))
}

pub fn remove_session(store: &SledStore, session: &Session) -> Result<(), AppError> {
    store.delete(&account_key(session))?;
    if store.get::<String>(KEY_ACTIVE_ACCOUNT)?.as_deref() == Some(session.uuid.as_str()) {
        store.delete(KEY_ACTIVE_ACCOUNT)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SledStore {
        let path = std::env::temp_dir().join(format!(
            "swift-launcher-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        SledStore::open_at(path).unwrap()
    }

    fn session(uuid: &str, username: &str) -> Session {
        Session {
            provider: crate::auth::AuthProvider::ElyBy,
            uuid: uuid.into(),
            username: username.into(),
            access_token: "access".into(),
            refresh_token: Some("client".into()),
            expires_at_unix: u64::MAX,
            avatar_url: None,
        }
    }

    #[test]
    fn list_sessions_ignores_active_uuid_value() {
        let store = store();
        let first = session("uuid-1", "Alpha");
        save_session(&store, &first).unwrap();

        let sessions = list_sessions(&store).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].uuid, "uuid-1");
    }

    #[test]
    fn removing_active_session_clears_active_pointer() {
        let store = store();
        let first = session("uuid-2", "Beta");
        save_session(&store, &first).unwrap();
        remove_session(&store, &first).unwrap();

        assert!(active_session(&store).unwrap().is_none());
    }
}
