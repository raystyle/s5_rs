---
id: ADR-0002
title: 文件道 object_store 与流道 s2-sdk 选型
status: accepted
date: 2026-10-11
deciders:
  - s5-rs 工位(研究档 research-s5-01 对照加本机实测,总台插队知会转呈)
superseded_by: null
supersedes: []
tags:
  - dependency
  - s3
  - s2
  - garage
---

# ADR-0002:文件道 object_store 与流道 s2-sdk 选型

关联 REQ-0001;对照与实测:docs/knowledge/research/S001。

## Context

文件道候选:object_store、aws-sdk-s3、rust-s3、rusty-s3(只签名)。
判据:IP 端点必须 path-style、明文 HTTP 端点、小文件 put/get/ls/del、
同二进制还要跑 s2 流道(tokio)、依赖面与 garage 兼容坑。流道无悬念:
官方 s2-sdk(lite 兼容 v0.22+),但要定端点与鉴权形态。

## Decision

1. 文件道:`object_store`(feature `aws`)。path-style 是默认(开关少
   一个)、`with_allow_http(true)` 显式放行、四动词与 trait 对齐、翻页
   内建、无 smithy 生成面;reqwest 与 s2-sdk 同 tokio。退路 rust-s3
   (更窄但 path-style 与自定义端点都有),触发条件:object_store 在
   garage 上签名不兼容且短修不动。
2. 不选 aws-sdk-s3:编译面最大,默认校验和(CRC32、aws-chunked、
   trailer)在 S3 兼容实现上有踩坑史;garage v1.1.0 无 2.x 的
   checksum-type 面。
3. 流道:`s2-sdk` 官方 crate,`S2Endpoints::for_endpoint` 单 URL 双用
   (lite 无子域路由),token 占位(见 ADR-0001)。
4. garage 边界内建假设:小文件单次 Put 不上分片;不承诺 Put 条件写
   (同键并发后写获胜);键建议 ASCII(编码坑见 S001)。

## Consequences

- 好:单运行时(tokio)双道;两 crate 均主流维护;错误面可归一
  (garage XML 抽 Code/Message,S2 ServerError 带 code)。
- 代价:object_store 拉 aws-base 加 reqwest(中等依赖面);版本锁
  0.14(edition 2024,需 rustc 1.85+,本机 1.98 无碍)。
- 复议钩子:若后续要大工件(分片)或 Put 条件写,升级到 garage 2.x
  核对后重开此 ADR。
