use crate::db::{DatabaseError, RuleExecutionSummary, SourceType};
use crate::pipeline::archive::ArchiveError;
use crate::pipeline::scan::ScanError;
use crate::storage::StorageError;
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageImportStage {
    /// 解压压缩包（仅压缩包输入）。
    Extracting,
    /// 扫描 PNG 文件。
    Scanning,
    /// 为身份键全新的图片计算内容哈希。
    Hashing,
    /// 读取元数据。
    Processing,
    /// 计算感知哈希（pHash）。
    PerceptualHashing,
    /// 把新图片复制/搬移进受管目录（落位副本）。
    Copying,
}

impl ImageImportStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extracting => "extracting",
            Self::Scanning => "scanning",
            Self::Hashing => "hashing",
            Self::Processing => "processing",
            Self::PerceptualHashing => "perceptualHashing",
            Self::Copying => "copying",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageImportProgress {
    pub stage: ImageImportStage,
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageImportOutcome {
    pub batch_id: i64,
    pub source_type: SourceType,
    pub total_found: usize,
    pub added: u64,
    pub skipped_existing: u64,
    pub skipped_content: u64,
    pub changed_existing: u64,
    /// 因读取失败或正负提示词均为空而拒绝入库的图片数。
    pub metadata_rejected: u64,
    /// 成功移动到用户配置目录的异常图片数。
    pub rejected_moved: u64,
    /// 未能移动到用户配置目录的异常图片数；这些图片仍不入库。
    pub rejected_move_failures: u64,
    pub rule_execution: RuleExecutionSummary,
    pub artist_prefix_enabled: bool,
    pub artist_prefix_scanned_rows: u64,
    pub artist_prefix_changed_rows: u64,
    pub artist_prefix_changed_fields: u64,
    pub artist_prefix_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImageUpdateOutcome {
    pub source_type: SourceType,
    pub total_found: usize,
    pub matched: u64,
    pub updated: u64,
    /// 由原路径身份键精确匹配的图片数。
    pub matched_by_identity: u64,
    /// 原路径失效后，由完整文件 SHA-256 唯一匹配并重新关联的图片数。
    pub relinked_by_content: u64,
    /// 文件字节变化后，由完整 NovelAI 元数据指纹唯一匹配并重新关联的图片数。
    pub relinked_by_metadata: u64,
    /// SHA 或元数据指向多条旧记录，未自动覆盖的图片数。
    pub ambiguous: u64,
    /// 来源中没有对应资料库身份键的图片；更新模式明确忽略，不追加。
    pub unmatched: u64,
    /// 已匹配但 PNG 元数据读取失败或正负提示词均为空；保留原行。
    pub metadata_rejected: u64,
    /// 已匹配且元数据有效，但受管原图副本刷新失败；保留原行。
    pub copy_failures: u64,
    pub rule_execution: RuleExecutionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingImageMatchKind {
    Identity,
    ContentHash,
    Metadata,
}

#[derive(Debug, Error)]
pub enum ImageImportError {
    #[error("导入文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Archive(#[from] ArchiveError),
    #[error("{0}")]
    Scan(#[from] ScanError),
    #[error("数据库写入失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("输入中没有找到 PNG 图片: {0}")]
    NoImagesFound(PathBuf),
    #[error("异常图片输出目录不能等于或位于导入文件夹内部: {0}")]
    RejectedDirectoryInsideInput(PathBuf),
    #[error("导入已被用户取消，未写入任何数据")]
    Cancelled,
    #[error("更新已被用户取消，未修改任何数据")]
    UpdateCancelled,
}
