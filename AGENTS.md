# S5

> S5(Shared Store & Streams):agent 跨机共享 CLI。文件道 S3 put/get/ls/del
> 加 bucket create 建桶,流道 S2 append/tail。全开放共享模型(桶与流
> 对全 agent 自由开放)。公开契约以类型签名与 `///` 契约注释为准。

## Commands

- `cargo test --locked` 全测试(单测加集成;流道集成需 `S5_TEST_S2_LITE=1` 加本地 `s2 lite`,缺件跳过;文件道集成需仓根 `.dev-key.txt` 加 garage 可达)
- `cargo fmt --all -- --check` 加 `cargo clippy --locked --all-targets` 提交前必跑(0 警告基线)
- `cargo aidoc` 后提交 `docs/aidoc/`(改 pub 项或 /// 后先重生成再提交;`cargo aidoc --check --strict` 漂移门禁)
- `cargo build` 出 `target/debug/s5`;真后端回环见 tests/cli.rs 门控注释
- `///` 契约注释覆盖公开项(missing_docs = deny);doctest 走集成测试 tests/ 承载
- `uv run --script .tools/md-ref-scan.py` 加 `.tools/md-heading-scan.py` 加 `.tools/mdcharlint.py` 加 `rumdl check .` md 四门禁
- `PEVO_CHECK_ALLOW='^docs/aidoc/' uv run /mnt/wsl/repos/project-evo/.tools/check.py .` 骨架合规自检(标准命令见 docs/README 末节)

## Must

- 改命令面同步:clap 注释加集成测试加 `--llms` 手册(活命令树渲染,无生成物落盘,漂移在源头不可能)
- 不可逆技术选择先立 `docs/knowledge/adr/`;新需求先立 `docs/knowledge/req/` REQ
- 事实性断言标六态(定义见 docs/README 写作规范节)
- 踩坑当场记 `docs/sources/diary/` 当天笔;选型与后端边界引 S001,不重研
- 一事一提交(feat/docs/fix/chore 前缀)
- 凭据纪律:命令面零凭据参数;密钥只经配置层与 S5_* 环境变量;`.dev-key.txt` 永不进 git
- wrt 上只读探测,不改 wrt 配置;生产 key 不经仓

## Must not

- 手改生成物(docs/aidoc 生成面)
- 另写第二真相(命令契约在 clap 与 tests,配置契约在 config.rs `///`)
- emoji、破折号、Unicode 箭头、非法全角(四类禁字,md-guard 守卫)
- 未经指示推远端、发版(发布面归总台)
- 在共享 basin 复用他人流(secret-guard-log 族勿动;s5 用 s5/ 前缀)

## Read first

- 检索阶梯(渐进披露,不整读):`s5 --llms`(CLI 手册投影)到 `docs/aidoc/llms.txt`(库投影)到模块 `///` 与源码;ADR 仅在改对应决策时读
- 文档地图:`docs/README.md`;需求队列:`docs/knowledge/req/README.md`;选型与后端边界:`docs/knowledge/research/S001`

## 环境

- 后端实况:garage v1.1.0 @ <http://192.168.88.2:3900>(region=garage,dev 桶 s5-dev,key 在仓根 .dev-key.txt);s2-lite @ <http://192.168.88.2:8090>(basin rx-home-basin,共享勿写)
- 本地流道测试件:s2 官方 CLI 起 `s2 lite --port 7171`,建 basin s5-dev-basin(名字至少 8 字节),`S5_TEST_S2_LITE=1 S5_TEST_S2_PORT=7171 cargo test`
- 评审闸门惯例:交付批经总台评审后合流
