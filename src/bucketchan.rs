//! 建桶道:garage Admin API 封装(全开放共享模型,`s5 bucket create`)。
//!
//! 全开放共享模型(REQ-0002):桶与流对全 agent 自由开放。建流已自由
//! (append 自动建);建桶走 garage admin 道:v1 Admin API(garage
//! v1.1.0 部署面)的 `POST /v1/bucket` 与 `POST /v1/bucket/allow`,
//! bearer token 收配置层 0600 密档(`s3.admin_endpoint`/
//! `s3.admin_token`),agent 命令面零凭据。建桶即授权配置层当前
//! s3 key 读写(非 owner),后续 put/get/ls/del 即用。garage 2.x 的
//! v2 admin API(`/v2/CreateBucket` 族)契约不同,当界外。
//!
//! 匿名语义边界:garage S3 面要 SigV4,由配置层吃掉(agent 永不接触
//! 钥);"全开放"落在 agent 使用面(共享配置即可自由读写),非网络面
//! 无鉴权;流道 lite 面无鉴权,天然匿名可用。

use serde_json::json;

use crate::config::Config;

/// 一处建桶道错误(信封 code 与 message 供 CLI 出 stderr)。
#[derive(Debug, thiserror::Error)]
pub enum BucketError {
    /// admin 道未配(admin_endpoint 或 admin_token 空)。
    #[error(
        "admin 道未配:s3.admin_endpoint 与 s3.admin_token 需先入配置层(config set 或 S5_S3_ADMIN_* 环境)"
    )]
    NotConfigured,
    /// 桶名不合法(空或空白)。
    #[error("桶名不合法: {0}")]
    InvalidName(String),
    /// admin API 拒绝(带归一 code 与后端消息)。
    #[error("{code}: {message}")]
    Admin {
        /// 归一错误码(AlreadyExists/AccessDenied/AdminApi)。
        code: String,
        /// 人读消息(后端消息优先)。
        message: String,
    },
    /// HTTP 传输层失败(网络不可达、超时等)。
    #[error("admin 请求失败: {0}")]
    Transport(String),
}

impl BucketError {
    /// 机器可读 code(信封 `code` 字段)。
    pub fn code(&self) -> &str {
        match self {
            BucketError::NotConfigured => "Config",
            BucketError::InvalidName(_) => "InvalidBucketName",
            BucketError::Admin { code, .. } => code,
            BucketError::Transport(_) => "AdminApi",
        }
    }

    /// 人读 message(信封 `message` 字段)。
    pub fn message(&self) -> String {
        match self {
            BucketError::Admin { message, .. } => message.clone(),
            other => other.to_string(),
        }
    }
}

/// 把 admin API 非 2xx 响应归一为信封错误。
///
/// garage v1.1.0 错误体是四字段 JSON,`code` 为字符串(如重名 409 带
/// `BucketAlreadyExists`,非法名 400 带 `InvalidRequest`;源码钉死,
/// 见 S002)。后端字符串 code 原码透传(合"后端原语义优先");体里无
/// code 时按状态兜底:401/403 归一 AccessDenied,其余记 AdminApi。
fn classify(status: u16, body: &str) -> BucketError {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let server_code = parsed
        .as_ref()
        .and_then(|v| v["code"].as_str())
        .map(str::to_string)
        .filter(|c| !c.is_empty());
    let server_message = parsed
        .as_ref()
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| {
            let t = body.trim();
            if t.len() > 200 {
                format!("{}…", &t[..200])
            } else {
                t.to_string()
            }
        });
    let code = server_code.unwrap_or_else(|| {
        if status == 401 || status == 403 {
            "AccessDenied".into()
        } else {
            "AdminApi".into()
        }
    });
    BucketError::Admin {
        code,
        message: format!("admin api {status}: {server_message}"),
    }
}

/// 建全局名桶并授权配置层当前 s3 key 读写。
///
/// 返回单行 ack 形 `{bucket, id, key_granted}`(key 未配时
/// `key_granted` 为 null,桶照建,授权面留待后补)。授权失败时桶已
/// 存在,message 前缀如实标注部分态。
///
/// # Errors
///
/// admin 道未配([`BucketError::NotConfigured`)、桶名空
/// ([`BucketError::InvalidName`)、admin API 拒绝与传输失败
/// ([`BucketError::Admin`]/[`BucketError::Transport`])。
pub async fn create(cfg: &Config, name: &str) -> Result<serde_json::Value, BucketError> {
    if name.trim().is_empty() {
        return Err(BucketError::InvalidName(name.into()));
    }
    if cfg.s3.admin_endpoint.trim().is_empty() || cfg.s3.admin_token.trim().is_empty() {
        return Err(BucketError::NotConfigured);
    }
    let base = cfg.s3.admin_endpoint.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();
    let auth_header = format!("Bearer {}", cfg.s3.admin_token);

    // 建桶:v1 POST /v1/bucket,body {"globalAlias": name}。
    let body = json!({ "globalAlias": name }).to_string();
    let resp = client
        .post(format!("{base}/v1/bucket"))
        .header("authorization", &auth_header)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| BucketError::Transport(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| BucketError::Transport(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(classify(status, &text));
    }
    let info: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        BucketError::Transport(format!(
            "建桶 2xx 但体非 JSON:{}",
            &text[..text.len().min(120)]
        ))
    })?;
    let id = info["id"].as_str().unwrap_or_default().to_string();

    // 授权:配置层带 access_key_id 则授 read+write(v1 POST
    // /v1/bucket/allow);未配 key 时跳过(key_granted 出 null)。
    let key = cfg.s3.access_key_id.trim().to_string();
    let granted = if key.is_empty() {
        serde_json::Value::Null
    } else {
        let allow = json!({
            "bucketId": id,
            "accessKeyId": key,
            "permissions": { "read": true, "write": true, "owner": false },
        })
        .to_string();
        let resp = client
            .post(format!("{base}/v1/bucket/allow"))
            .header("authorization", &auth_header)
            .header("content-type", "application/json")
            .body(allow)
            .send()
            .await
            .map_err(|e| BucketError::Transport(e.to_string()))?;
        let astatus = resp.status().as_u16();
        let atext = resp
            .text()
            .await
            .map_err(|e| BucketError::Transport(e.to_string()))?;
        if !(200..300).contains(&astatus) {
            let mut e = classify(astatus, &atext);
            if let BucketError::Admin { message, .. } = &mut e {
                *message = format!("桶 {name} 已建(id {id})但授权 {key} 失败:{message}");
            }
            return Err(e);
        }
        serde_json::Value::String(key)
    };

    Ok(json!({ "bucket": name, "id": id, "key_granted": granted }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(admin_ep: &str, admin_tk: &str, key: &str) -> Config {
        let mut c = Config::default();
        c.s3.admin_endpoint = admin_ep.into();
        c.s3.admin_token = admin_tk.into();
        c.s3.access_key_id = key.into();
        c
    }

    #[tokio::test]
    async fn create_requires_admin_config() {
        let e = create(&cfg("http://x:1", "", "GKk"), "b")
            .await
            .unwrap_err();
        assert_eq!(e.code(), "Config", "token 空即未配:{e}");
        let e = create(&cfg("", "t", "GKk"), "b").await.unwrap_err();
        assert_eq!(e.code(), "Config", "端点空即未配:{e}");
    }

    #[tokio::test]
    async fn create_rejects_blank_name() {
        let e = create(&cfg("http://x:1", "t", "GKk"), "  ")
            .await
            .unwrap_err();
        assert_eq!(e.code(), "InvalidBucketName", "空白名拒:{e}");
    }

    #[test]
    fn classify_maps_statuses() {
        assert_eq!(
            classify(
                409,
                r#"{"code":"BucketAlreadyExists","message":"Bucket already exists"}"#
            )
            .code(),
            "BucketAlreadyExists",
            "后端字符串 code 原码透传"
        );
        assert_eq!(
            classify(
                400,
                r#"{"code":"InvalidRequest","message":"Bad request: bad name"}"#
            )
            .code(),
            "InvalidRequest"
        );
        assert_eq!(classify(403, "forbidden").code(), "AccessDenied");
        assert_eq!(
            classify(400, r#"{"code":null,"message":"bad"}"#).code(),
            "AdminApi"
        );
        let e = classify(400, "not json");
        assert!(e.message().contains("not json"), "非 JSON 体入消息:{e}");
    }
}
