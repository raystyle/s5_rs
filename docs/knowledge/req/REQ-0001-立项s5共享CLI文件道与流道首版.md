---
id: REQ-0001
title: 立项 s5 共享 CLI,文件道与流道首版
status: implemented
priority: must
trace: cargo test --locked 全绿(单测 10 加集成 9,含 garage s5-dev 真回环与本地 s2-lite 真回环)
---

# REQ-0001:立项 s5 共享 CLI,文件道与流道首版

## Scenario

agent 跨机协作需要共享内容:文件与工件走 S3 对象交换,轨迹流与事件流走
S2 耐久追加流;家族第四件(herdr 信号、hst 会话态、ark 运行时、s5 共享
内容)。发起单:s5-01(2026-10-11,总台)。

## Criteria

- [x] 仓骨架:README、AGENTS.md、.gitignore(含 .dev-key.txt)、clap
  命令面(家族形:clap derive、help 模板、裸调用退 0)
- [x] 文件道首版:s5 put/get/ls/del(object_store,garage path-style,
  region 对齐)
- [x] 流道首版:s5 append/tail(s2-sdk 封装,basin/stream API 调研落
  docs/knowledge/research/S001;tail 有界缺省、-f 跟随退 130、--seq 续读
  越尾空出;append 自动建流、批拆 1000 条/768 KiB、空输入不发请求)
- [x] 零感知凭据:命令面零凭据参数;TOML 0600 密档(`s5 config init/
  show/set/path`)加 S5_S3_*/S5_S2_* 环境覆盖;show 掩码不回显明文
- [x] 测试全绿:单测加对 s5-dev 桶真 PUT/GET 回环集成测试;流道对本地
  s2-lite 真回环(S5_TEST_S2_LITE=1 门控);cargo fmt 加 clippy 0 警告
- [x] 手册面:s5 --llms(markdown)与 --llms --json(机器形)由活命令树
  渲染,无生成物落盘

## Trace 回填

- 全绿证据:2026-10-11 `cargo test --locked` 19 项全过(10 单测加 9 集成,
  含 s3_roundtrip_on_dev_bucket 与 s2_append_tail_and_streams_on_local_lite);
  `cargo clippy --locked --all-targets` 0 警告。
- 选型与边界档:docs/knowledge/research/S001;零感知凭据裁定:
  ADR-0001;选型裁定:ADR-0002。
