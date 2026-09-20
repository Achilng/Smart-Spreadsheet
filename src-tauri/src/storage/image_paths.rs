//! Resolve the same readable-image candidates used by hash backfills.
use super::DataDirectory;
use crate::db::ContentHashCandidate;
use std::io;
use std::path::PathBuf;

pub(super) fn resolve_hash_candidate_path(
    directory: &DataDirectory,
    candidate: &ContentHashCandidate,
) -> Result<PathBuf, io::Error> {
    if let Some(path) = nonempty(candidate.image_path.as_deref()).map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }
    if let Some(relative) = nonempty(candidate.stored_image_path.as_deref()) {
        let path = directory.root().join(relative);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("第 {} 行没有可读图片", candidate.row_id),
    ))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
