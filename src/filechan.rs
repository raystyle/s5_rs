//! 文件道:S3 对象交换(garage 兼容后端,put/get/ls/del)。
//!
//! 选型 object_store 的 aws feature(RS 见 docs/knowledge/research/S001):
//! IP 端点必须 path-style(object_store 默认即是),HTTP 端点显式
//! `allow_http`;region 必须等于服务端 `s3_region`(SigV4 scope 敏感)。
//! garage v1.1.0 边界:无版本化(删即真删、覆盖不留旧版)、无 Put 条件
//! 写(同键并发后写获胜,不承诺原子创建)、小文件走单次 Put 不上分片、
//! 列举由 object_store 内部跟 continuation 页。

use bytes::Bytes;
use chrono::{DateTime, Utc};
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use serde::Serialize;

use crate::config::Config;

/// 一处文件道错误(S3 语义错带 code,供 CLI 单行 JSON 出 stderr)。
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// 后端返回的 S3 语义错误(NoSuchKey 等;garage 以 XML code 回)。
    #[error("{code}: {message}")]
    S3 {
        /// S3 错误码。
        code: String,
        /// 人读消息。
        message: String,
    },
    /// 本地 IO(读源文件、写落盘目标)。
    #[error("本地 IO 失败 {path}: {source}")]
    Io {
        /// 出错路径。
        path: String,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// 客户端构造失败(端点不合法等)。
    #[error("S3 客户端构造失败: {0}")]
    Build(String),
    /// 键不合法(空或以 `/` 结尾的目录形)。
    #[error("键不合法: {0}")]
    InvalidKey(String),
}

impl FileError {
    /// 机器可读 code(信封 `code` 字段)。
    pub fn code(&self) -> &str {
        match self {
            FileError::S3 { code, .. } => code,
            FileError::Io { .. } => "LocalIo",
            FileError::Build(_) => "BuildError",
            FileError::InvalidKey(_) => "InvalidKey",
        }
    }

    /// 人读 message(信封 `message` 字段;不带 code 前缀)。
    pub fn message(&self) -> String {
        match self {
            FileError::S3 { message, .. } => message.clone(),
            other => other.to_string(),
        }
    }
}

/// 从 garage/S3 的 XML 错误体抽 `<Code>`/`<Message>`(无则 None)。
fn extract_xml(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].to_string())
}

/// 从 object_store 错误归一化:变体精确归类优先,XML 体里能抽到
/// Code/Message 则用后端原语义,否则全文入 message、code 记 S3Error。
fn norm(e: object_store::Error) -> FileError {
    let text = e.to_string();
    if let (Some(code), Some(message)) = (extract_xml(&text, "Code"), extract_xml(&text, "Message"))
    {
        return FileError::S3 { code, message };
    }
    match e {
        object_store::Error::NotFound { path, .. } => FileError::S3 {
            code: "NoSuchKey".into(),
            message: format!("键不存在 {path}"),
        },
        object_store::Error::AlreadyExists { path, .. } => FileError::S3 {
            code: "AlreadyExists".into(),
            message: format!("键已存在 {path}"),
        },
        object_store::Error::Precondition { path, .. } => FileError::S3 {
            code: "PreconditionFailed".into(),
            message: format!("条件不满足 {path}"),
        },
        object_store::Error::NotSupported { source } => FileError::S3 {
            code: "NotSupported".into(),
            message: source.to_string(),
        },
        other => FileError::S3 {
            code: "S3Error".into(),
            message: other.to_string(),
        },
    }
}

/// 由配置建 S3 客户端(path-style 默认、HTTP 放行、region 对齐服务端)。
fn store(cfg: &Config) -> Result<AmazonS3, FileError> {
    AmazonS3Builder::new()
        .with_endpoint(&cfg.s3.endpoint)
        .with_region(&cfg.s3.region)
        .with_bucket_name(&cfg.s3.bucket)
        .with_allow_http(cfg.s3.endpoint.starts_with("http://"))
        .with_access_key_id(&cfg.s3.access_key_id)
        .with_secret_access_key(&cfg.s3.secret_access_key)
        .build()
        .map_err(|e| FileError::Build(format!("{e}")))
}

/// put 的产物行。
#[derive(Debug, Serialize)]
pub struct PutOutcome {
    /// 远端键。
    pub key: String,
    /// 上传字节数。
    pub size: u64,
    /// 服务端 ETag(往返对账用,不承诺内容 MD5 算法形)。
    pub etag: Option<String>,
}

/// ls 的对象行。
#[derive(Debug, Serialize)]
pub struct ObjectRow {
    /// 对象键。
    pub key: String,
    /// 字节数。
    pub size: u64,
    /// 最后修改时间(RFC3339)。
    pub last_modified: DateTime<Utc>,
}

/// 键守卫:非空、不以 `/` 结尾(目录形键对三件套语义无意义)。
fn check_key(key: &str) -> Result<(), FileError> {
    if key.is_empty() {
        return Err(FileError::InvalidKey("空键".into()));
    }
    if key.ends_with('/') {
        return Err(FileError::InvalidKey(format!("键以 / 结尾(目录形):{key}")));
    }
    Ok(())
}

/// 读源:`-` 表 stdin,否则本地文件路径;整读进内存(小文件面)。
fn read_source(source: &str) -> Result<Bytes, FileError> {
    if source == "-" {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|source| FileError::Io {
                path: "<stdin>".into(),
                source,
            })?;
        Ok(Bytes::from(buf))
    } else {
        std::fs::read(source)
            .map(Bytes::from)
            .map_err(|source| FileError::Io {
                path: source.to_string(),
                source,
            })
    }
}

/// `s5 put <file> <key>`:本地文件(或 `-` stdin)整读单次上传。
///
/// # Errors
///
/// 键不合法、本地源读失败、客户端构造失败或后端 S3 语义错。
pub async fn put(cfg: &Config, source: &str, key: &str) -> Result<PutOutcome, FileError> {
    check_key(key)?;
    let data = read_source(source)?;
    let store = store(cfg)?;
    let path = Path::from(key);
    let size = data.len() as u64;
    let res = store.put(&path, data.into()).await.map_err(norm)?;
    Ok(PutOutcome {
        key: key.to_string(),
        size,
        etag: res.e_tag,
    })
}

/// `s5 get <key>`:取回对象字节(调用方决定落 stdout 还是文件)。
///
/// # Errors
///
/// 键不合法、客户端构造失败、键不存在(NoSuchKey)或传输错。
pub async fn get(cfg: &Config, key: &str) -> Result<Bytes, FileError> {
    check_key(key)?;
    let store = store(cfg)?;
    store
        .get(&Path::from(key))
        .await
        .map_err(norm)?
        .bytes()
        .await
        .map_err(norm)
}

/// `s5 ls [prefix]`:前缀列举(字面前缀非 glob;空前缀全列),键序升序。
///
/// # Errors
///
/// 客户端构造失败、列举翻页中传输或 S3 语义错。
pub async fn ls(cfg: &Config, prefix: &str) -> Result<Vec<ObjectRow>, FileError> {
    use futures_util::TryStreamExt;
    let store = store(cfg)?;
    let prefix = if prefix.is_empty() {
        None
    } else {
        Some(Path::from(prefix))
    };
    let metas: Vec<object_store::ObjectMeta> = store
        .list(prefix.as_ref())
        .try_collect()
        .await
        .map_err(norm)?;
    let mut rows: Vec<ObjectRow> = metas
        .into_iter()
        .map(|m| ObjectRow {
            // as_ref() 出原文键(Display 形是 URL 编码 wire 形;put/get 与
            // ls 同串往返,agent 不该见到两副面孔)。
            key: m.location.as_ref().to_string(),
            size: m.size,
            last_modified: m.last_modified,
        })
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(rows)
}

/// `s5 del <key>`:删除(S3 语义幂等:键不存在同样报成功)。
///
/// # Errors
///
/// 键不合法、客户端构造失败或传输错(键不存在不报错)。
pub async fn del(cfg: &Config, key: &str) -> Result<String, FileError> {
    check_key(key)?;
    let store = store(cfg)?;
    store.delete(&Path::from(key)).await.map_err(norm)?;
    Ok(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_extraction() {
        let e = FileError::S3 {
            code: "NoSuchKey".into(),
            message: "The specified key does not exist.".into(),
        };
        assert_eq!(e.code(), "NoSuchKey");
        assert_eq!(
            e.to_string(),
            "NoSuchKey: The specified key does not exist."
        );
    }

    #[test]
    fn xml_code_message_extraction() {
        let body = r#"<?xml version="1.0"?><Error><Code>AccessDenied</Code><Message>Access Denied.</Message><Resource>/x</Resource></Error>"#;
        assert_eq!(extract_xml(body, "Code").as_deref(), Some("AccessDenied"));
        assert_eq!(
            extract_xml(body, "Message").as_deref(),
            Some("Access Denied.")
        );
        assert_eq!(extract_xml("no xml here", "Code"), None);
        // 嵌套错文含多段 XML 时取首段。
        let nested = "wrap <Code>First</Code> mid <Code>Second</Code>";
        assert_eq!(extract_xml(nested, "Code").as_deref(), Some("First"));
    }
}
