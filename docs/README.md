# docs 地图

> 三层知识结构(sources 依据、knowledge 定义、operations 手册)加
> aidoc 生成投影。单一权威源原则:命令契约在 clap 与 tests,标记契约
> 在源码 `///`,本目录是其投影与依据档。

| 路径 | 层 | 内容 |
|---|---|---|
| [aidoc/](aidoc/) | 投影 | cargo aidoc 生成(禁手改,漂移门禁) |
| [knowledge/req/](knowledge/req/README.md) | 定义 | 需求登记(REQ) |
| [knowledge/adr/](knowledge/adr/README.md) | 定义 | 架构决策(ADR) |
| [knowledge/research/](knowledge/research/README.md) | 定义 | 研究档案(S 编号,六态) |
| [operations/](operations/README.md) | 手册 | 工具链与工作流标准(G 编号) |
| [sources/diary/](sources/diary/README.md) | 依据 | 执行日记 |
| [sources/acceptance/](sources/acceptance/README.md) | 依据 | 验收实弹档 |

## 写作规范

- 四类禁字:emoji、破折号、Unicode 箭头、非法全角(md-guard 守卫)
- 事实性断言标六态(定义见 research 档头);实证滥用即未完成
- 研究档结构:背景、关键结论、现状或实测、踩坑沉淀、待办

## 骨架合规自检

```bash
PEVO_CHECK_ALLOW='^docs/aidoc/' uv run /mnt/wsl/repos/project-evo/.tools/check.py .
```

豁免语义:aidoc 是生成物渲染格式(cargo-aidoc 列表项自带连接号分隔
符),源头在 `///` 与生成器,不在手写面;真门禁是 md 四件(md-ref-scan
加 md-heading-scan 加 mdcharlint 加 rumdl)与 aidoc 漂移守卫。
