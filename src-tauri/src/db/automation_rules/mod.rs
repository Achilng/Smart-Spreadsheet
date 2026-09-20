//! SQLite persistence and transactional execution of automation rules.
pub(crate) mod execution;
pub(crate) mod repository;
#[cfg(test)]
mod tests;
pub(crate) mod transfer;

pub use crate::automation::*;
pub use crate::storage::rule_files::{
    parse_automation_rule_text, read_automation_rule_file, write_automation_rule_file,
};
