# s5-cli::llms

`s5 --llms` 手册面:由活命令树渲染,无生成物落盘(家族形,对齐
hst_rs ADR-0005:命令树是真源,手册是投影,漂移在源头不可能)。

双形态:裸 `--llms` 出 markdown 手册(至多 120 行),`--llms --json`
出机器形({name, version, description, commands[]} 递归)。

agent 面旗标裁剪记录(对齐 evo-doc doc-cli 七件族,s5 取三裁四,
裁剪在案):采用 `--help`、`--llms`、`--json`(与 --llms 合用);
裁剪 `--format`/`--filter-output`/`--full-output`/`--schema`,因 s5
输出本就类型化(stdout 行式 JSON/JSONL 纯数据,错误 stderr 单行
JSON 信封),无多格式族与信封剥壳需求;退出码 1 通吃业务与用法错
(家族形 hst REQ-032),不取 grep 三态。漂移守卫由集成测试
llms_manual_and_machine_forms 穷尽对锁命令集。

## Functions

- `llm_machine_form` — `s5 --llms --json` 机器形态:命令树递归,组到叶。
- `render_llms` — 渲染 markdown 手册(子命令表逐行带用法串与说明)。

