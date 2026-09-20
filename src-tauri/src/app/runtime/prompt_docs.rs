use super::{AppRuntime, AppRuntimeError};
use crate::storage::{PromptDocAsset, PromptDocDetail, PromptDocSummary};
use std::path::Path;

impl AppRuntime {
    pub(crate) fn list_prompt_docs(&self) -> Result<Vec<PromptDocSummary>, AppRuntimeError> {
        Ok(self.active_directory()?.list_prompt_docs()?)
    }

    pub(crate) fn create_prompt_doc(
        &self,
        title: &str,
    ) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self.active_directory()?.create_prompt_doc(title)?)
    }

    pub(crate) fn load_prompt_doc(&self, doc_id: &str) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self.active_directory()?.load_prompt_doc(doc_id)?)
    }

    pub(crate) fn save_prompt_doc(
        &self,
        doc_id: &str,
        title: &str,
        content: &serde_json::Value,
        plain_text: &str,
    ) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .save_prompt_doc(doc_id, title, content, plain_text)?)
    }

    pub(crate) fn delete_prompt_doc(&self, doc_id: &str) -> Result<(), AppRuntimeError> {
        Ok(self.active_directory()?.delete_prompt_doc(doc_id)?)
    }

    pub(crate) fn import_prompt_doc_image_from_path(
        &self,
        doc_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<PromptDocAsset, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .import_prompt_doc_image_from_path(doc_id, path)?)
    }

    pub(crate) fn import_prompt_doc_image_bytes(
        &self,
        doc_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<PromptDocAsset, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .import_prompt_doc_image_bytes(doc_id, file_name, bytes)?)
    }
}
