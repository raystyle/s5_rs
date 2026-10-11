# s5-cli::config

配置层:零感知凭据的落点(ADR-0001)。

garage 实测不支持匿名访问,凭据(S3 key 与 S2 token)收进本层;agent
命令面零凭据参数。配置文件 TOML 0600(`S5_CONFIG` 覆盖路径,缺省
XDG `s5/config.toml`),不进 git;环境变量逐键覆盖(测试与无头场景)。

读序:环境变量 > 配置文件 > 随包缺省(内网 dev 指向;生产切换只改配置)。

## Types

- `Config` — 顶层配置:两道后端一段一节。
- `ConfigError` — 一处配置错误(路径、解析、未知键)。
- `S2Section` — 流道(S2)段:端点、basin 与占位 token。
- `S3Section` — 文件道(S3)段:endpoint、region、桶与静态密钥。

