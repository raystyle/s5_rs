# Requirements 索引

> 需求登记:新需求先立 REQ 再实现,实现回填 trace(测试或验收命令)。
> 状态机 draft 到 implemented 或 rejected。模板见 ark_rs 同目录形。

| id | 状态 | 标题 | trace |
|---|---|---|---|
| REQ-0001 | implemented | 立项 s5 共享 CLI 文件道与流道首版 | cargo test --locked 全绿 19 项(含 s5-dev 真回环与本地 s2-lite 真回环)加 clippy 0 警告 |
| REQ-0002 | implemented | 全开放共享模型与建桶动词 bucket create | cargo test --locked 全绿 24 项;admin 真回环门控缺 token 跳过,wire 面实测 3903 AccessDenied 信封 |
