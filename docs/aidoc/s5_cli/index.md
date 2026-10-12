# s5-cli 0.1.0

S5(Shared Store & Streams):agent 跨机共享 CLI 库面。

家族第四件:herdr 管信号,hst 管会话态,ark 管运行时,s5 管共享内容。
两道后端、一层配置、零感知凭据(命令面不出现密钥与端点,见 ADR-0001):

- 文件道([`filechan`]):S3 对象交换(garage 兼容后端),put/get/ls/del
- 建桶道([`bucketchan`]):garage admin API 建桶与授权(全开放共享)
- 流道([`streamchan`]):S2 耐久追加流(s2-lite 兼容后端),append/tail
- 配置层([`config`]):TOML 0600 密档,环境变量覆盖,凭据只经此道

契约以本库 `///` 注释与集成测试为准;CLI 手册面由活命令树渲染(`s5 --llms`)。

## Modules

- [`bucketchan`](bucketchan.md): 建桶道:garage Admin API 封装(全开放共享模型,`s5 bucket create`)。
- [`config`](config.md): 配置层:零感知凭据的落点(ADR-0001)。
- [`filechan`](filechan.md): 文件道:S3 对象交换(garage 兼容后端,put/get/ls/del)。
- [`llms`](llms.md): `s5 --llms` 手册面:由活命令树渲染,无生成物落盘(家族形,对齐
- [`streamchan`](streamchan.md): 流道:S2 耐久追加流(s2-lite 兼容后端,append/tail/streams)。

