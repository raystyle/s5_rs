# s5-cli::bucketchan

建桶道:garage Admin API 封装(全开放共享模型,`s5 bucket create`)。

全开放共享模型(REQ-0002):桶与流对全 agent 自由开放。建流已自由
(append 自动建);建桶走 garage admin 道:v1 Admin API(garage
v1.1.0 部署面)的 `POST /v1/bucket` 与 `POST /v1/bucket/allow`,
bearer token 收配置层 0600 密档(`s3.admin_endpoint`/
`s3.admin_token`),agent 命令面零凭据。建桶即授权配置层当前
s3 key 读写(非 owner),后续 put/get/ls/del 即用。garage 2.x 的
v2 admin API(`/v2/CreateBucket` 族)契约不同,当界外。

匿名语义边界:garage S3 面要 SigV4,由配置层吃掉(agent 永不接触
钥);"全开放"落在 agent 使用面(共享配置即可自由读写),非网络面
无鉴权;流道 lite 面无鉴权,天然匿名可用。

## Functions

- `create` — 建全局名桶并授权配置层当前 s3 key 读写。

## Types

- `BucketError` — 一处建桶道错误(信封 code 与 message 供 CLI 出 stderr)。

