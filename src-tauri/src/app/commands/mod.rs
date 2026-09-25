//! Tauri IPC adapters, grouped by the operation they expose.
pub(crate) mod artists;
pub(crate) mod style_extraction;
pub(crate) mod web_tools;
pub(crate) mod automation;
pub(crate) mod compare;
mod dto;
pub(crate) mod duplicates;
pub(crate) mod editing;
pub(crate) mod exports;
pub(crate) mod files;
pub(crate) mod groups;
pub(crate) mod images;
pub(crate) mod imports;
pub(crate) mod library;
pub(crate) mod maintenance;
pub(crate) mod prompt_docs;
pub(crate) mod quick_edit;
pub(crate) mod rows;
pub(crate) mod tags;
#[cfg(test)]
mod tests;
pub(crate) mod windows;

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
