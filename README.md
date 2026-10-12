# S5

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## 项目介绍

S5(Shared Store & Streams)是 agent 跨机共享 CLI:文件与工件走 S3 对象
交换(put/get/ls/del),轨迹流与事件流走 S2 耐久追加流(append/tail)。
家族第四件:herdr 管信号,hst 管会话态,ark 管运行时,s5 管共享内容。

全开放共享模型(REQ-0002):桶与流对全 agent 自由开放,`s5 bucket
create` 自助建桶(建桶即授权当前 key 读写),流 append 自动建。匿名
语义边界:garage S3 面要 SigV4,由配置层吃掉(agent 永不接触钥);
开放落在使用面,非网络面无鉴权;流道 lite 面无鉴权。

后端(内网):文件道 garage v1.1.0(S3 兼容,path-style);流道 s2-lite
(s2 官方自托管服务,basin/stream 面见 docs/knowledge/research/S001)。

零感知凭据(ADR-0001):命令面零凭据参数,S3 key 与 admin token、S2
token 全在配置层(TOML 0600 密档加环境变量覆盖);内网边界即信任边界。

## 部署

```bash
cargo install --path .   # 或家族分发(ark install s5,首版后接)
```

## 配置

```bash
s5 config init                        # 落 0600 缺省配置(dev 指向)
s5 config set s3.access_key_id GK…    # 写 dev key(总台发,.dev-key.txt 同源)
s5 config set s3.secret_access_key …
s5 config set s3.admin_token …        # 建桶道 bearer(garage admin,可选)
s5 config show                        # 生效配置(密钥掩码,不回显明文)
```

环境变量逐键覆盖(无头与测试):`S5_S3_ENDPOINT`、`S5_S3_REGION`、
`S5_S3_BUCKET`、`S5_S3_ACCESS_KEY_ID`、`S5_S3_SECRET_ACCESS_KEY`、
`S5_S3_ADMIN_ENDPOINT`、`S5_S3_ADMIN_TOKEN`、`S5_S2_ENDPOINT`、
`S5_S2_BASIN`、`S5_S2_TOKEN`;`S5_CONFIG` 指配置文件路径。

## 使用方法

文件道(S3,键建议 ASCII;put/get 同串往返一致):

```bash
echo "内容" > a.txt
s5 put a.txt rounds/2026-10-11/a.txt   # 上传(源 - 表 stdin)
s5 get rounds/2026-10-11/a.txt         # 取回(原始字节到 stdout)
s5 get rounds/2026-10-11/a.txt -o b.txt
s5 ls rounds/                          # 前缀列(JSONL;空前缀全列)
s5 del rounds/2026-10-11/a.txt         # 幂等删
s5 bucket create s5-shared             # 自助建桶(授权当前 key 读写)
```

流道(S2,一行一条记录;流名可含 / 那是名字字符;共享 basin 里建议
s5/ 前缀命名,勿动他人流):

```bash
printf '事件一\n事件二\n' | s5 append s5/轨迹流   # 逐行追加,流不存在自动建
s5 tail s5/轨迹流                        # 最近 20 条(有界退出)
s5 tail s5/轨迹流 -n 100                 # 最近 100 条(单批上限 1000)
s5 tail s5/轨迹流 --seq 42               # 从序号 42 续读(游标续读)
s5 tail s5/轨迹流 -f                     # 跟随到 SIGINT(退 130)
s5 streams s5/                           # 列流(前缀可选)
```

agent 手册面:`s5 --llms`(markdown)与 `s5 --llms --json`(机器形),
由活命令树渲染,无生成物落盘。

## 输出与退出码

stdout 行式 JSON/JSONL(put/del/append 单行,ls/tail/streams 逐行,get
出原始字节);错误 stderr 单行 JSON `{"code","message"}`。退出码:0 成功
(裸 s5 打帮助亦 0;tail 空流或游标在尾上亦 0),1 业务错(含用法错),
130 tail -f 被 SIGINT 打断。

记录行形态:`{"seq_num","timestamp","body"}`;body 非 UTF-8 字节时改出
`body_b64`(base64,不损坏二进制记录)。

## 开发

见 [AGENTS.md](AGENTS.md) 命令面;知识结构见 [docs/README.md](docs/README.md)。
