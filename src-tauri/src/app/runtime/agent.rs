use super::{AppRuntime, ensure_startup_valid};
use crate::db::Database;

impl AppRuntime {
    /// Run agent database work under the runtime lock; `mutate` invalidates the query cache.
    pub(crate) fn agent_db<T>(
        &self,
        mutate: bool,
        f: impl FnOnce(&mut Database) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.lock_state().map_err(|error| error.to_string())?;
        ensure_startup_valid(&state).map_err(|error| error.to_string())?;
        let database = state.database().map_err(|error| error.to_string())?;
        let result = f(database)?;
        if mutate {
            database.bump_data_version();
        }
        Ok(result)
    }

    /// A stable identity of the open library, so a conversation never writes into another one.
    pub(crate) fn agent_library_id(&self) -> Result<String, String> {
        Ok(self.llm_context()?.1)
    }
}
