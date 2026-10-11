//! 配置层:零感知凭据的落点(ADR-0001)。
//!
//! garage 实测不支持匿名访问,凭据(S3 key 与 S2 token)收进本层;agent
//! 命令面零凭据参数。配置文件 TOML 0600(`S5_CONFIG` 覆盖路径,缺省
//! XDG `s5/config.toml`),不进 git;环境变量逐键覆盖(测试与无头场景)。
//!
//! 读序:环境变量 > 配置文件 > 随包缺省(内网 dev 指向;生产切换只改配置)。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 一处配置错误(路径、解析、未知键)。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// 配置文件 TOML 解析失败。
    #[error("配置解析失败 {path}: {source}")]
    Parse {
        /// 配置文件路径。
        path: PathBuf,
        /// 底层 toml 错误。
        source: toml::de::Error,
    },
    /// 配置文件读失败(权限、IO)。
    #[error("配置读取失败 {path}: {source}")]
    Io {
        /// 配置文件路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// `config set` 收到不在白名单里的点径键。
    #[error("未知配置键 {0}(可设键:s3.endpoint/s3.region/s3.bucket/s3.access_key_id/s3.secret_access_key/s2.endpoint/s2.basin/s2.token)")]
    UnknownKey(String),
}

/// 文件道(S3)段:endpoint、region、桶与静态密钥。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct S3Section {
    /// S3 端点(garage v1.1.0 @ wrt host 网;path-style)。
    pub endpoint: String,
    /// SigV4 签名 region,必须等于服务端 `s3_region`(garage 缺省 `garage`)。
    pub region: String,
    /// 缺省桶(dev 面 s5-dev;生产 herdr-flywheel 首版后接)。
    pub bucket: String,
    /// 访问密钥 ID(GK 起;零感知凭据只经配置层)。
    pub access_key_id: String,
    /// 访问密钥私钥(不进命令面、不进 git、不回显明文)。
    pub secret_access_key: String,
}

/// 流道(S2)段:端点、basin 与占位 token。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct S2Section {
    /// S2 端点(s2-lite HTTP 面;与 garage 对象口不同源)。
    pub endpoint: String,
    /// 缺省 basin(lite 单端点无子域路由,basin 名经请求头承载)。
    pub basin: String,
    /// 访问 token(lite 无鉴权面,占位 `ignored`;边界当内网信任)。
    pub token: String,
}

/// 顶层配置:两道后端一段一节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// 文件道(S3)段。
    pub s3: S3Section,
    /// 流道(S2)段。
    pub s2: S2Section,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            s3: S3Section {
                endpoint: "http://192.168.88.2:3900".into(),
                region: "garage".into(),
                bucket: "s5-dev".into(),
                access_key_id: String::new(),
                secret_access_key: String::new(),
            },
            s2: S2Section {
                endpoint: "http://192.168.88.2:8090".into(),
                basin: "rx-home-basin".into(),
                token: "ignored".into(),
            },
        }
    }
}

impl Config {
    /// 配置文件路径:`S5_CONFIG` 覆盖,缺省 XDG 配置目录 `s5/config.toml`。
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("S5_CONFIG") {
            return PathBuf::from(p);
        }
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("s5").join("config.toml")
    }

    /// 读序:环境变量逐键覆盖 > 配置文件(缺省文件可缺)> 随包缺省。
    ///
    /// 配置文件不存在不报错(全缺省态可跑,凭据空时后端报 403)。
    ///
    /// # Errors
    ///
    /// 配置文件存在但读失败或 TOML 解析失败。
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::path();
        let mut cfg = if path.is_file() {
            let raw = fs::read_to_string(&path).map_err(|source| ConfigError::Io {
                path: path.clone(),
                source,
            })?;
            toml::from_str(&raw).map_err(|source| ConfigError::Parse {
                path: path.clone(),
                source,
            })?
        } else {
            Self::default()
        };
        cfg.apply_env();
        Ok(cfg)
    }

    /// 环境变量逐键覆盖(`S5_S3_*`/`S5_S2_*`);空串视作显式置空。
    fn apply_env(&mut self) {
        let vars: [(&str, &mut String); 8] = [
            ("S5_S3_ENDPOINT", &mut self.s3.endpoint),
            ("S5_S3_REGION", &mut self.s3.region),
            ("S5_S3_BUCKET", &mut self.s3.bucket),
            ("S5_S3_ACCESS_KEY_ID", &mut self.s3.access_key_id),
            ("S5_S3_SECRET_ACCESS_KEY", &mut self.s3.secret_access_key),
            ("S5_S2_ENDPOINT", &mut self.s2.endpoint),
            ("S5_S2_BASIN", &mut self.s2.basin),
            ("S5_S2_TOKEN", &mut self.s2.token),
        ];
        for (name, slot) in vars {
            if let Ok(v) = std::env::var(name) {
                *slot = v;
            }
        }
    }

    /// 以 0600 权限原子落盘(先写临时文件再改名;父目录按需建)。
    ///
    /// # Errors
    ///
    /// 目录创建、写入、权限设置或改名任一步 IO 失败。
    ///
    /// # Panics
    ///
    /// 配置结构序列化失败(结构恒可序列化,理论不可达)。
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|source| ConfigError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let raw = toml::to_string_pretty(self).expect("Config 恒可序列化");
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, raw.as_bytes()).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|source| {
                ConfigError::Io {
                    path: tmp.clone(),
                    source,
                }
            })?;
        }
        fs::rename(&tmp, &path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }

    /// `config set` 的点径白名单写入口;返回写后的新值。
    ///
    /// # Errors
    ///
    /// 键不在白名单([`ConfigError::UnknownKey`])。
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key {
            "s3.endpoint" => self.s3.endpoint = value.into(),
            "s3.region" => self.s3.region = value.into(),
            "s3.bucket" => self.s3.bucket = value.into(),
            "s3.access_key_id" => self.s3.access_key_id = value.into(),
            "s3.secret_access_key" => self.s3.secret_access_key = value.into(),
            "s2.endpoint" => self.s2.endpoint = value.into(),
            "s2.basin" => self.s2.basin = value.into(),
            "s2.token" => self.s2.token = value.into(),
            other => return Err(ConfigError::UnknownKey(other.into())),
        }
        Ok(())
    }

    /// 展示形:密钥类字段掩码(前 4 加省略号);agent 面永不回显明文。
    pub fn masked(&self) -> serde_json::Value {
        fn mask(v: &str) -> String {
            if v.is_empty() {
                "(空)".into()
            } else {
                format!("{}…", v.chars().take(4).collect::<String>())
            }
        }
        serde_json::json!({
            "path": Self::path(),
            "s3": {
                "endpoint": self.s3.endpoint,
                "region": self.s3.region,
                "bucket": self.s3.bucket,
                "access_key_id": mask(&self.s3.access_key_id),
                "secret_access_key": mask(&self.s3.secret_access_key),
            },
            "s2": {
                "endpoint": self.s2.endpoint,
                "basin": self.s2.basin,
                "token": mask(&self.s2.token),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip_and_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.s3.region, "garage");
        assert_eq!(cfg.s2.token, "ignored");
        let raw = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&raw).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn set_whitelist_and_unknown() {
        let mut cfg = Config::default();
        cfg.set("s3.bucket", "herdr-flywheel").unwrap();
        assert_eq!(cfg.s3.bucket, "herdr-flywheel");
        assert!(matches!(
            cfg.set("s3.evil", "x"),
            Err(ConfigError::UnknownKey(_))
        ));
    }

    #[test]
    fn env_overrides_file_and_default() {
        let unique = format!("s5-envtest-{}", std::process::id());
        let dir = std::env::temp_dir().join(&unique);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        fs::write(
            &p,
            "[s3]\nendpoint = \"http://file:1\"\nregion = \"r\"\nbucket = \"b\"\naccess_key_id = \"a\"\nsecret_access_key = \"s\"\n[s2]\nendpoint = \"http://f2:2\"\nbasin = \"bb\"\ntoken = \"t\"\n",
        )
        .unwrap();
        // SAFETY(测试):子进程环境隔离由测试框架保证,不并发读同键。
        std::env::set_var("S5_CONFIG", &p);
        std::env::set_var("S5_S3_BUCKET", "env-wins");
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.s3.endpoint, "http://file:1", "文件值生效");
        assert_eq!(cfg.s3.bucket, "env-wins", "环境变量覆盖文件");
        std::env::remove_var("S5_S3_BUCKET");
        std::env::remove_var("S5_CONFIG");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn masked_never_leaks_secret() {
        let mut cfg = Config::default();
        cfg.s3.secret_access_key = "3f75b653supersecret".into();
        let shown = cfg.masked().to_string();
        assert!(!shown.contains("supersecret"), "明文不得回显:{shown}");
        assert!(shown.contains("3f75…"), "前 4 位掩码形:{shown}");
    }
}
