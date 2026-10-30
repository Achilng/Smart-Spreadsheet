use super::AppRuntime;
use crate::db::{Database, DatabaseError, llm::*};

impl AppRuntime {
    pub(crate) fn llm_context(&self) -> Result<(String, String, Settings), String> {
        let mut state = self.lock_state().map_err(|e| e.to_string())?;
        let root = state
            .active
            .as_ref()
            .ok_or("尚未打开资料库")?
            .root()
            .to_string_lossy()
            .into_owned();
        let db = state.database().map_err(|e| e.to_string())?;
        let identity = format!("{}|{}", root, db.llm_id().map_err(|e| e.to_string())?);
        let mut settings = db.llm_settings().map_err(|e| e.to_string())?;
        // Migrate while holding the same database lock as reads and writes.
        if crate::app::llm::migrate_keys(&mut settings) {
            db.set_setting(SETTINGS_KEY, &json(&settings).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        Ok((root, identity, settings))
    }
    pub(crate) fn llm_db<T>(
        &self,
        library_id: &str,
        f: impl FnOnce(&mut Database) -> Result<T, DatabaseError>,
    ) -> Result<T, String> {
        let mut state = self.lock_state().map_err(|e| e.to_string())?;
        let root = state
            .active
            .as_ref()
            .ok_or("尚未打开资料库")?
            .root()
            .to_string_lossy()
            .into_owned();
        let db = state.database().map_err(|e| e.to_string())?;
        if format!("{}|{}", root, db.llm_id().map_err(|e| e.to_string())?) != library_id {
            return Err("资料库已切换".into());
        }
        f(db).map_err(|e| e.to_string())
    }
}
