# Changelog

## Unreleased

- 全开放共享模型(REQ-0002):`s5 bucket create 桶名` 自助建桶(garage
  v1 admin 道,建桶即授权配置层当前 key 读写;admin 凭据入 0600 密档,
  老档缺键兼容);流 append 自动建保持;模型与匿名语义边界文档化进
  `--llms` 手册面。

## 0.1.0

> 2026-10-11。首版(REQ-0001,单 s5-01):

- 文件道:S3 put/get/ls/del(object_store 0.14,garage path-style)
- 流道:S2 append/tail/streams(s2-sdk 0.35,自动建流、批拆、有界尾读、
  `-f` 跟随退 130、`--seq` 续读越尾空出)
- 零感知凭据配置层(ADR-0001):TOML 0600 密档、S5_* 环境覆盖、show 掩码
- agent 手册面:`--llms` 与 `--llms --json`(活命令树渲染)
- 测试:19 项全绿(含 garage s5-dev 真回环与本地 s2-lite 真回环)
