# s5-cli::streamchan

流道:S2 耐久追加流(s2-lite 兼容后端,append/tail/streams)。

官方 `s2-sdk`(tokio),端点经 [`S2Endpoints::for_endpoint`] 单 URL 双用
(lite 无子域路由,basin 由 SDK 经 `S2-Basin` 头承载);lite 无鉴权面,
token 占位 `ignored`(ADR-0001 零感知凭据)。

语义要点(RS 见 docs/knowledge/research/S001):
- 追加:一行一条记录;单批上限 1000 条或 1 MiB 计量字节,超出拆批,
  ack 汇总为首批 start 与末批 end/tail;空输入不发请求。
- 自动建流:basin 未开 `create_stream_on_append`(本机实况),首条
  append 前建流;`already exists` 视为成功,不触碰既有流配置。
- 读回:`tail` 默认有界退出(对齐 unix tail / kubectl logs 先例),
  `-f` 才跟随;`--seq` 给续读游标;越过 tail 的起点是「游标在尾上」
  而非故障,退 0 空出。
- 输出 JSONL:记录一行一对象;body 是 UTF-8 时 `body` 字符串,否则
  `body_b64` base64(记录本体是任意字节,不做有损转义)。

[S2Endpoints::for_endpoint]: s2_sdk::types::S2Endpoints::for_endpoint

## Functions

- `append` — `s5 append <stream>`:逐行成记录追加;自动建流;返回 None 表零输入。
- `follow` — `s5 tail -f`(跟随):先出历史(`-n`/`--seq` 同 tail),再进读会话到 SIGINT。
- `list_streams` — `s5 streams`:列配置 basin 下的流(可选字面前缀),名序。
- `tail` — `s5 tail <stream>`(有界):读最近 N 条或 `--seq S` 起的全部已写记录。

## Types

- `AppendOutcome` — append 的产物行(AppendAck 原字段名,多批汇总为首批 start 与末批 end/tail)。
- `FollowEnd` — follow 的收束因。
- `PosOut` — 位置三件(append ack 与游标面的原子形)。
- `RecordRow` — tail 的记录行(JSONL;UTF-8 走 body,任意字节走 body_b64)。
- `StreamError` — 一处流道错误(服务端语义错带 code,供 CLI 单行 JSON 出 stderr)。
- `StreamRow` — streams 的流行。

## Constants

- `BATCH_MAX_BYTES` — 单批计量字节上限(S2 平台限制 1 MiB;留余量取 768 KiB)。
- `BATCH_MAX_RECORDS` — 单批记录条数上限(S2 平台限制 1000)。

