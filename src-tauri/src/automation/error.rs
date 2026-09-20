use crate::db::DatabaseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutomationRuleError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] DatabaseError),
    #[error("规则数据读写失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("规则名称不能为空")]
    EmptyName,
    #[error("规则至少需要一个条件组")]
    EmptyConditionSet,
    #[error("第 {0} 个条件组没有条件")]
    EmptyConditionGroup(usize),
    #[error("规则至少需要一个执行任务")]
    EmptyActions,
    #[error("规则字段不能为空: {0}")]
    EmptyValue(&'static str),
    #[error("数值区间条件缺少结束值")]
    MissingRangeEnd,
    #[error("无效的正则表达式: {0}")]
    InvalidRegex(String),
    #[error("不存在的规则 ID: {0}")]
    RuleNotFound(i64),
    #[error("规则顺序必须包含当前全部规则且不能重复")]
    InvalidOrder,
    #[error("不存在的目标分组 ID: {0}")]
    MissingTargetGroup(i64),
    #[error("规则配置无效: {0}")]
    InvalidDefinition(String),
    #[error("规则文件读写失败: {0}")]
    FileIo(#[from] std::io::Error),
    #[error("无效的规则文件: {0}")]
    InvalidRuleFile(String),
}

impl From<rusqlite::Error> for AutomationRuleError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(DatabaseError::Sqlite(value))
    }
}
