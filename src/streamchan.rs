//! 流道:S2 耐久追加流(s2-lite 兼容后端,append/tail/streams)。
//!
//! 官方 `s2-sdk`(tokio),端点经 [`S2Endpoints::for_endpoint`] 单 URL 双用
//! (lite 无子域路由,basin 由 SDK 经 `S2-Basin` 头承载);lite 无鉴权面,
//! token 占位 `ignored`(ADR-0001 零感知凭据)。
//!
//! 语义要点(RS 见 docs/knowledge/research/S001):
//! - 追加:一行一条记录;单批上限 1000 条或 1 MiB 计量字节,超出拆批,
//!   ack 汇总为首批 start 与末批 end/tail;空输入不发请求。
//! - 自动建流:basin 未开 `create_stream_on_append`(本机实况),首条
//!   append 前建流;`already exists` 视为成功,不触碰既有流配置。
//! - 读回:`tail` 默认有界退出(对齐 unix tail / kubectl logs 先例),
//!   `-f` 才跟随;`--seq` 给续读游标;越过 tail 的起点是「游标在尾上」
//!   而非故障,退 0 空出。
//! - 输出 JSONL:记录一行一对象;body 是 UTF-8 时 `body` 字符串,否则
//!   `body_b64` base64(记录本体是任意字节,不做有损转义)。
//!
//! [S2Endpoints::for_endpoint]: s2_sdk::types::S2Endpoints::for_endpoint

use base64::Engine as _;
use futures_util::StreamExt;
use s2_sdk::error::ErrorCode;
use s2_sdk::types::{
    AppendInput, AppendRecord, AppendRecordBatch, BasinName, CreateStreamInput,
    ListAllStreamsInput, ReadFrom, ReadInput, ReadSessionConfig, ReadStart, S2Config, S2Endpoints,
    StreamName,
};
use s2_sdk::S2;
use serde::Serialize;

use crate::config::Config;

/// 单批记录条数上限(S2 平台限制 1000)。
pub const BATCH_MAX_RECORDS: usize = 1000;
/// 单批计量字节上限(S2 平台限制 1 MiB;留余量取 768 KiB)。
pub const BATCH_MAX_BYTES: usize = 768 * 1024;

/// 一处流道错误(服务端语义错带 code,供 CLI 单行 JSON 出 stderr)。
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// 后端返回的 S2 语义错误(stream_not_found 等)。
    #[error("{code}: {message}")]
    S2 {
        /// S2 错误码。
        code: String,
        /// 人读消息。
        message: String,
    },
    /// 传输/网络层错(连不上、超时、连接重置)。
    #[error("S2 连接失败: {0}")]
    Transport(String),
    /// 输入校验错(流名不合法、非 UTF-8 行)。
    #[error("输入不合法: {0}")]
    Validation(String),
}

impl StreamError {
    /// 机器可读 code(信封 `code` 字段)。
    pub fn code(&self) -> &str {
        match self {
            StreamError::S2 { code, .. } => code,
            StreamError::Transport(_) => "Transport",
            StreamError::Validation(_) => "ValidationError",
        }
    }

    /// 人读 message(信封 `message` 字段;不带 code 前缀)。
    pub fn message(&self) -> String {
        match self {
            StreamError::S2 { message, .. } => message.clone(),
            other => other.to_string(),
        }
    }
}

impl From<s2_sdk::error::RequestError> for StreamError {
    fn from(e: s2_sdk::error::RequestError) -> Self {
        match e {
            s2_sdk::error::RequestError::Server(se) => StreamError::S2 {
                code: se.code,
                message: se.message,
            },
            other => StreamError::Transport(other.to_string()),
        }
    }
}

impl From<s2_sdk::types::ValidationError> for StreamError {
    fn from(e: s2_sdk::types::ValidationError) -> Self {
        StreamError::Validation(e.to_string())
    }
}

impl From<s2_sdk::error::AppendError> for StreamError {
    fn from(e: s2_sdk::error::AppendError) -> Self {
        match e {
            s2_sdk::error::AppendError::Request(re) => re.into(),
            other => StreamError::S2 {
                code: "AppendFailed".into(),
                message: other.to_string(),
            },
        }
    }
}

impl From<s2_sdk::error::ReadError> for StreamError {
    fn from(e: s2_sdk::error::ReadError) -> Self {
        match e {
            s2_sdk::error::ReadError::Request(re) => re.into(),
            other => StreamError::S2 {
                code: "ReadFailed".into(),
                message: other.to_string(),
            },
        }
    }
}

impl From<s2_sdk::error::ReadSessionError> for StreamError {
    fn from(e: s2_sdk::error::ReadSessionError) -> Self {
        match e {
            s2_sdk::error::ReadSessionError::Read(re) => re.into(),
            other => StreamError::Transport(other.to_string()),
        }
    }
}

/// 由配置建 S2 客户端(单端点双用;token 占位)。
fn client(cfg: &Config) -> Result<S2, StreamError> {
    let endpoints = S2Endpoints::for_endpoint(&cfg.s2.endpoint)
        .map_err(|e| StreamError::Validation(format!("s2.endpoint 不合法: {e}")))?;
    let config = S2Config::new(cfg.s2.token.clone()).with_endpoints(endpoints);
    S2::new(config).map_err(|e| StreamError::Transport(format!("S2 客户端构造失败: {e}")))
}

/// 位置三件(append ack 与游标面的原子形)。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PosOut {
    /// 序号(服务端分配,严格递增)。
    pub seq_num: u64,
    /// 时间戳(毫秒)。
    pub timestamp: u64,
}

impl From<s2_sdk::types::StreamPosition> for PosOut {
    fn from(p: s2_sdk::types::StreamPosition) -> Self {
        Self {
            seq_num: p.seq_num,
            timestamp: p.timestamp,
        }
    }
}

/// append 的产物行(AppendAck 原字段名,多批汇总为首批 start 与末批 end/tail)。
#[derive(Debug, Serialize)]
pub struct AppendOutcome {
    /// 本组首条序号位置。
    pub start: PosOut,
    /// 本组末条之后的位置。
    pub end: PosOut,
    /// 追加后流尾。
    pub tail: PosOut,
}

/// tail 的记录行(JSONL;UTF-8 走 body,任意字节走 body_b64)。
#[derive(Debug, Serialize)]
pub struct RecordRow {
    /// 序号。
    pub seq_num: u64,
    /// 时间戳(毫秒)。
    pub timestamp: u64,
    /// 记录本体(UTF-8 时出现)。
    pub body: Option<String>,
    /// 记录本体 base64(非 UTF-8 字节时出现,此时 body 为 null)。
    pub body_b64: Option<String>,
}

impl RecordRow {
    /// 由 SDK 序号记录构造(本体按 UTF-8 可解性分流)。
    fn from_sequenced(r: &s2_sdk::types::SequencedRecord) -> Self {
        match std::str::from_utf8(&r.body) {
            Ok(text) => Self {
                seq_num: r.seq_num,
                timestamp: r.timestamp,
                body: Some(text.to_string()),
                body_b64: None,
            },
            Err(_) => Self {
                seq_num: r.seq_num,
                timestamp: r.timestamp,
                body: None,
                body_b64: Some(base64::engine::general_purpose::STANDARD.encode(&r.body)),
            },
        }
    }
}

/// streams 的流行。
#[derive(Debug, Serialize)]
pub struct StreamRow {
    /// 流名。
    pub name: String,
    /// 建流时间(RFC3339,服务端原串)。
    pub created_at: String,
}

/// 流不存在的判定(自动建流与续读空出共享)。
fn is_code(e: &StreamError, code: ErrorCode) -> bool {
    match e {
        StreamError::S2 { code: c, .. } => *c == code.to_string(),
        _ => false,
    }
}

/// 读起点:`--seq S`(含)优先,否则尾偏移 N(从尾往回数)。
fn read_start(seq: Option<u64>, tail_offset: u64) -> ReadStart {
    let from = match seq {
        Some(s) => ReadFrom::SeqNum(s),
        None => ReadFrom::TailOffset(tail_offset),
    };
    ReadStart::new().with_from(from)
}

/// `s5 append <stream>`:逐行成记录追加;自动建流;返回 None 表零输入。
///
/// 建流只认 `ResourceAlreadyExists` 为已存在(不 reconfigure 既有流);
/// basin 未开 `create_stream_on_append` 的实况由这条兜住。
///
/// # Errors
///
/// 流名不合法、建流被拒(非 already-exists)、批量校验或追加传输错。
pub async fn append(
    cfg: &Config,
    stream: &str,
    lines: Vec<String>,
) -> Result<Option<AppendOutcome>, StreamError> {
    if lines.is_empty() {
        return Ok(None);
    }
    let s2 = client(cfg)?;
    let basin_name: BasinName = basin_name(cfg)?;
    let stream_name: StreamName = stream
        .parse()
        .map_err(|e: s2_sdk::types::ValidationError| {
            StreamError::Validation(format!("流名不合法 {stream}: {e}"))
        })?;
    let basin = s2.basin(basin_name);
    match basin
        .create_stream(CreateStreamInput::new(stream_name.clone()))
        .await
    {
        Ok(_) => {}
        Err(e) => {
            let se: StreamError = e.into();
            if !is_code(&se, ErrorCode::ResourceAlreadyExists) {
                return Err(se);
            }
        }
    }
    let stream = basin.stream(stream_name);

    let mut ack: Option<AppendOutcome> = None;
    for chunk in chunk_lines(&lines) {
        let records = chunk
            .iter()
            .map(|l| AppendRecord::new(l.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let batch = AppendRecordBatch::try_from_iter(records)?;
        let out = stream.append(AppendInput::new(batch)).await?;
        let merged = AppendOutcome {
            start: out.start.into(),
            end: out.end.into(),
            tail: out.tail.into(),
        };
        ack = Some(match ack {
            None => merged,
            Some(prev) => AppendOutcome {
                start: prev.start,
                end: merged.end,
                tail: merged.tail,
            },
        });
    }
    Ok(ack)
}

/// 批拆分(纯函数):单批至多 1000 条且累计本体字节至多 768 KiB。
fn chunk_lines(lines: &[String]) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut cur_bytes = 0usize;
    for line in lines {
        let b = line.len();
        if !cur.is_empty() && (cur.len() >= BATCH_MAX_RECORDS || cur_bytes + b > BATCH_MAX_BYTES) {
            chunks.push(std::mem::take(&mut cur));
            cur_bytes = 0;
        }
        cur_bytes += b;
        cur.push(line.clone());
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// `s5 tail <stream>`(有界):读最近 N 条或 `--seq S` 起的全部已写记录。
///
/// 单批服务端上限 1000 条/1 MiB;`-n` 夹到该量级(20 缺省);`--seq`
/// 在尾上或越尾(ReadUnwritten)是游标已到位而非故障,退空。
///
/// # Errors
///
/// 流名不合法、流不存在或传输错(`--seq` 越尾不算错,退空)。
pub async fn tail(
    cfg: &Config,
    stream: &str,
    n: u64,
    seq: Option<u64>,
) -> Result<Vec<RecordRow>, StreamError> {
    let stream = stream_client(cfg, stream).await?;
    let input = ReadInput::new().with_start(read_start(seq, n));
    match stream.read(input).await {
        Ok(batch) => Ok(batch
            .records
            .iter()
            .map(RecordRow::from_sequenced)
            .collect()),
        Err(s2_sdk::error::ReadError::ReadUnwritten(_)) => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

/// follow 的收束因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowEnd {
    /// SIGINT 收到(调用方退 130,对齐 shell 惯例)。
    Interrupted,
}

/// `s5 tail -f`(跟随):先出历史(`-n`/`--seq` 同 tail),再进读会话到 SIGINT。
///
/// 无新数据时靠服务端会话等待(wait 语义),不做客户端忙等;历史与跟随
/// 之间以「已见最大序号 + 1」续接,不重不漏。
///
/// # Errors
///
/// 流名不合法、流不存在或读会话中途传输错。
pub async fn follow<F>(
    cfg: &Config,
    stream: &str,
    n: u64,
    seq: Option<u64>,
    mut on_record: F,
) -> Result<FollowEnd, StreamError>
where
    F: FnMut(&RecordRow),
{
    let stream_client = stream_client(cfg, stream).await?;
    // 历史腿:与 tail 同一起点,记录最大序号。
    let history = match stream_client
        .read(ReadInput::new().with_start(read_start(seq, n)))
        .await
    {
        Ok(batch) => batch
            .records
            .iter()
            .map(RecordRow::from_sequenced)
            .collect::<Vec<_>>(),
        Err(s2_sdk::error::ReadError::ReadUnwritten(_)) => vec![],
        Err(e) => return Err(e.into()),
    };
    let next = history.last().map(|r| r.seq_num + 1).or(seq).unwrap_or(0);
    for row in &history {
        on_record(row);
    }

    // 跟随腿:从 next 起的会话;会话本身长连,直到 ctrl_c。
    let mut session = stream_client
        .read_session(
            ReadInput::new().with_start(ReadStart::new().with_from(ReadFrom::SeqNum(next))),
            ReadSessionConfig::default(),
        )
        .await
        .map_err(StreamError::from)?;
    loop {
        tokio::select! {
            batch = session.next() => {
                let Some(batch) = batch else { break };
                let batch = batch.map_err(|e: s2_sdk::error::ReadSessionError| {
                    StreamError::Transport(e.to_string())
                })?;
                for r in &batch.records {
                    let row = RecordRow::from_sequenced(r);
                    on_record(&row);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                return Ok(FollowEnd::Interrupted);
            }
        }
    }
    // 会话对端关闭(服务端停机等):如实退,不当 SIGINT。
    Ok(FollowEnd::Interrupted)
}

/// `s5 streams`:列配置 basin 下的流(可选字面前缀),名序。
///
/// # Errors
///
/// basin 名不合法、前缀不合法或列举传输错。
pub async fn list_streams(
    cfg: &Config,
    prefix: Option<&str>,
) -> Result<Vec<StreamRow>, StreamError> {
    let s2 = client(cfg)?;
    let basin_name = basin_name(cfg)?;
    let basin = s2.basin(basin_name);
    let mut input = ListAllStreamsInput::new();
    if let Some(p) = prefix {
        input.prefix = p
            .parse()
            .map_err(|e| StreamError::Validation(format!("流名前缀不合法 {p}: {e}")))?;
    }
    let mut rows: Vec<StreamRow> = Vec::new();
    let mut streaming = basin.list_all_streams(input);
    while let Some(info) = streaming.next().await {
        let info = info?;
        if info.deleted_at.is_some() {
            continue;
        }
        rows.push(StreamRow {
            name: info.name.to_string(),
            created_at: info.created_at.to_string(),
        });
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rows)
}

/// basin 名解析(配置层一处)。
fn basin_name(cfg: &Config) -> Result<BasinName, StreamError> {
    cfg.s2
        .basin
        .parse()
        .map_err(|e: s2_sdk::types::ValidationError| {
            StreamError::Validation(format!("s2.basin 不合法 {}: {e}", cfg.s2.basin))
        })
}

/// 建流客户端(append/tail/follow 共用)。
async fn stream_client(cfg: &Config, stream: &str) -> Result<s2_sdk::S2Stream, StreamError> {
    let s2 = client(cfg)?;
    let stream_name: StreamName = stream
        .parse()
        .map_err(|e: s2_sdk::types::ValidationError| {
            StreamError::Validation(format!("流名不合法 {stream}: {e}"))
        })?;
    Ok(s2.basin(basin_name(cfg)?).stream(stream_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_respects_both_limits() {
        let lines: Vec<String> = (0..2500).map(|i| format!("line-{i}-body")).collect();
        let chunks = chunk_lines(&lines);
        assert_eq!(chunks.len(), 3, "2500 条按 1000 上限拆三批");
        assert_eq!(chunks[0].len(), 1000);
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 2500);

        // 字节上限:单条 300KiB,三条即越 768KiB。
        let big = "x".repeat(300 * 1024);
        let chunks = chunk_lines(&[big.clone(), big.clone(), big.clone()]);
        assert_eq!(chunks.len(), 2, "字节维拆批");
        assert_eq!(chunks[0].len(), 2);

        // 空输入零批。
        assert!(chunk_lines(&[]).is_empty());
        // 单条越限仍自成一批(交由 SDK 的记录级校验如实报错)。
        let huge = "y".repeat(BATCH_MAX_BYTES + 1);
        assert_eq!(chunk_lines(&[huge]).len(), 1);
    }

    #[test]
    fn record_row_utf8_vs_binary() {
        let text = s2_sdk::types::SequencedRecord::from_parts(1, 2, vec![], "文本");
        let row = RecordRow::from_sequenced(&text);
        assert_eq!(row.body.as_deref(), Some("文本"));
        assert!(row.body_b64.is_none());

        let bin = s2_sdk::types::SequencedRecord::from_parts(1, 2, vec![], vec![0xff_u8, 0xfe]);
        let row = RecordRow::from_sequenced(&bin);
        assert!(row.body.is_none());
        assert_eq!(row.body_b64.as_deref(), Some("//4="));
    }

    #[test]
    fn error_code_surface() {
        let e = StreamError::S2 {
            code: "stream_not_found".into(),
            message: "stream `x` in basin `b` not found".into(),
        };
        assert_eq!(e.code(), "stream_not_found");
        assert_eq!(
            e.to_string(),
            "stream_not_found: stream `x` in basin `b` not found"
        );
        assert!(is_code(&e, ErrorCode::StreamNotFound));
    }
}
