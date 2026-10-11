# s5-cli::filechan

文件道:S3 对象交换(garage 兼容后端,put/get/ls/del)。

选型 object_store 的 aws feature(RS 见 docs/knowledge/research/S001):
IP 端点必须 path-style(object_store 默认即是),HTTP 端点显式
`allow_http`;region 必须等于服务端 `s3_region`(SigV4 scope 敏感)。
garage v1.1.0 边界:无版本化(删即真删、覆盖不留旧版)、无 Put 条件
写(同键并发后写获胜,不承诺原子创建)、小文件走单次 Put 不上分片、
列举由 object_store 内部跟 continuation 页。

## Functions

- `del` — `s5 del <key>`:删除(S3 语义幂等:键不存在同样报成功)。
- `get` — `s5 get <key>`:取回对象字节(调用方决定落 stdout 还是文件)。
- `ls` — `s5 ls [prefix]`:前缀列举(字面前缀非 glob;空前缀全列),键序升序。
- `put` — `s5 put <file> <key>`:本地文件(或 `-` stdin)整读单次上传。

## Types

- `FileError` — 一处文件道错误(S3 语义错带 code,供 CLI 单行 JSON 出 stderr)。
- `ObjectRow` — ls 的对象行。
- `PutOutcome` — put 的产物行。

