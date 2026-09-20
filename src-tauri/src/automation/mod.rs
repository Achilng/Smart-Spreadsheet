//! Automation rule definitions and in-memory validation, matching and actions.
pub(crate) mod actions;
pub(crate) mod error;
pub(crate) mod matching;
pub(crate) mod model;
pub(crate) mod text;
pub(crate) mod validation;
pub use error::AutomationRuleError;
pub use model::*;
