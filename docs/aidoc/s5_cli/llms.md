# s5-cli::llms

`s5 --llms` 手册面:由活命令树渲染,无生成物落盘(家族形,对齐
hst_rs ADR-0005:命令树是真源,手册是投影,漂移在源头不可能)。

双形态:裸 `--llms` 出 markdown 手册(至多 120 行),`--llms --json`
出机器形({name, version, description, commands[]} 递归)。

## Functions

- `llm_machine_form` — `s5 --llms --json` 机器形态:命令树递归,组到叶。
- `render_llms` — 渲染 markdown 手册(子命令表逐行带用法串与说明)。

