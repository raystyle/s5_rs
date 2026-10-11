//! S5(Shared Store & Streams):agent 跨机共享 CLI 库面。
//!
//! 家族第四件:herdr 管信号,hst 管会话态,ark 管运行时,s5 管共享内容。
//! 两道后端、一层配置、零感知凭据(命令面不出现密钥与端点,见 ADR-0001):
//!
//! - 文件道([`filechan`]):S3 对象交换(garage 兼容后端),put/get/ls/del
//! - 流道([`streamchan`]):S2 耐久追加流(s2-lite 兼容后端),append/tail
//! - 配置层([`config`]):TOML 0600 密档,环境变量覆盖,凭据只经此道
//!
//! 契约以本库 `///` 注释与集成测试为准;CLI 手册面由活命令树渲染(`s5 --llms`)。

pub mod config;
pub mod filechan;
pub mod llms;
pub mod streamchan;

pub use config::Config;
