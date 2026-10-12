//! `s5 --llms` 手册面:由活命令树渲染,无生成物落盘(家族形,对齐
//! hst_rs ADR-0005:命令树是真源,手册是投影,漂移在源头不可能)。
//!
//! 双形态:裸 `--llms` 出 markdown 手册(至多 120 行),`--llms --json`
//! 出机器形({name, version, description, commands[]} 递归)。
//!
//! agent 面旗标裁剪记录(对齐 evo-doc doc-cli 七件族,s5 取三裁四,
//! 裁剪在案):采用 `--help`、`--llms`、`--json`(与 --llms 合用);
//! 裁剪 `--format`/`--filter-output`/`--full-output`/`--schema`,因 s5
//! 输出本就类型化(stdout 行式 JSON/JSONL 纯数据,错误 stderr 单行
//! JSON 信封),无多格式族与信封剥壳需求;退出码 1 通吃业务与用法错
//! (家族形 hst REQ-032),不取 grep 三态。漂移守卫由集成测试
//! llms_manual_and_machine_forms 穷尽对锁命令集。

use clap::Command;

/// 渲染 markdown 手册(子命令表逐行带用法串与说明)。
pub fn render_llms(root: &Command) -> String {
    let mut rows: Vec<(String, String)> = Vec::new();
    walk(root, String::new(), &mut rows);
    let mut table = String::new();
    for (usage, about) in &rows {
        let about = if about.is_empty() {
            "(见子命令)".into()
        } else {
            about.clone()
        };
        table.push_str(&format!("| `{usage}` | {about} |\n"));
    }
    format!(
        "# s5 {ver}\n\n> S5(Shared Store & Streams):agent 跨机共享 CLI。文件道 put/get/ls/del 加 bucket create(S3 对象交换与自助建桶),流道 append/tail(S2 耐久流);全开放共享模型,凭据全在配置层,命令面零凭据参数(`s5 config` 管)。手册由活命令树渲染;契约以 clap 帮助与集成测试为准。\n\n## 读序\n\n常见任务直达:传文件 `s5 put`,取文件 `s5 get`,发轨迹 `echo x | s5 append 流名`,续读轨迹 `s5 tail 流名 --seq N`。本手册机器形:`s5 --llms --json`。契约权威:`s5 --help` 与仓内 tests/。\n\n## 共享模型\n\n全开放:桶与流对全 agent 自由开放。`s5 bucket create 桶名` 自助建桶\n(garage admin 道,建桶即授权配置层当前 key 读写);流 append 自动建,\n无需预建。共享钥零感知:所有钥(S3 key、admin token、S2 token)只经\n`s5 config` 与 S5_* 环境入配置层,agent 永不接触。匿名语义边界:\ngarage S3 面要 SigV4,由配置层吃掉;开放落在使用面(共享配置即自由\n读写),非网络面无鉴权;流道 lite 面无鉴权,天然匿名可用。\n\n## 子命令表\n\n| 命令 | 说明 |\n| --- | --- |\n{table}## 通用旗标\n\n| 旗标 | 说明 |\n| --- | --- |\n| `--llms` | 本手册;配 `--json` 出机器形态 |\n| `--json` | 与 `--llms` 合用出机器形手册(其余命令输出本就是行式 JSON/JSONL,无需此旗标) |\n| `--help` / `--version` | 帮助与版本 |\n\n## 退出码\n\n| 码 | 义 |\n| --- | --- |\n| 0 | 成功(裸 s5 打印帮助亦退 0;tail 空流/游标在尾上退 0 空出) |\n| 1 | 业务失败(后端语义错、本地 IO、配置缺失、用法错) |\n| 130 | `tail -f` 被 SIGINT 打断(shell 惯例) |\n\n## 输出契约\n\nstdout 行式 JSON/JSONL(put/del/append/bucket create 单行,ls/tail/streams 逐行,get 出原始字节);错误 stderr 单行 JSON {{code, message}};密钥类不回显明文。\n\n## 常用例\n\n```bash\ns5 config init                      # 落 0600 配置(缺省 dev 指向)\ns5 config set s3.access_key_id GK…  # 写 key(再 set secret)\ns5 bucket create s5-shared          # 自助建桶(授权当前 key 读写)\ns5 put 工件.zip rounds/今天/工件.zip # 传文件\ns5 get rounds/今天/工件.zip -o 工件.zip\ns5 ls rounds/                        # 前缀列\ns5 del rounds/今天/工件.zip\nprintf 'line1\\nline2\\n' | s5 append s5/轨迹流   # 逐行成记录\ns5 tail s5/轨迹流 -n 20              # 最近 20 条(有界)\ns5 tail s5/轨迹流 --seq 42          # 从序号 42 续读\ns5 tail s5/轨迹流 -f                 # 跟随到 SIGINT(退 130)\ns5 streams s5/                      # 列流(前缀可选)\ns5 --llms --json                    # 机器形手册\n```\n",
        ver = env!("CARGO_PKG_VERSION"),
        table = table,
    )
}

/// `s5 --llms --json` 机器形态:命令树递归,组到叶。
pub fn llm_machine_form(root: &Command) -> serde_json::Value {
    fn cmd_node(cmd: &Command) -> serde_json::Value {
        let name = cmd.get_name().to_string();
        let description = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
        let subs: Vec<serde_json::Value> = cmd
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .map(cmd_node)
            .collect();
        let mut v = serde_json::json!({ "name": name, "description": description });
        if !subs.is_empty() {
            v["subcommands"] = serde_json::Value::Array(subs);
        }
        v
    }
    let subs: Vec<serde_json::Value> = root
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .map(cmd_node)
        .collect();
    serde_json::json!({
        "name": "s5",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "S5(Shared Store & Streams):agent 跨机共享 CLI(S3 文件道 + S2 流道)",
        "commands": subs,
    })
}

/// 递归收集 (usage, about),父命令可裸调时也记一行。
fn walk(cmd: &Command, prefix: String, rows: &mut Vec<(String, String)>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            format!("s5 {}", sub.get_name())
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
        rows.push((synopsis(sub, &path), about));
        if sub.has_subcommands() {
            walk(sub, path, rows);
        }
    }
}

/// 命令表用法串:路径加位置参数加可选旗标(全局与隐藏旗标不入串)。
fn synopsis(cmd: &Command, path: &str) -> String {
    let mut s = String::from(path);
    for arg in cmd.get_positionals() {
        let name = arg
            .get_value_names()
            .map(|v| v.join(" "))
            .unwrap_or_default();
        let form = if arg.is_required_set() {
            format!("<{name}>")
        } else {
            format!("[{name}]")
        };
        s.push(' ');
        s.push_str(&form);
    }
    let flags: Vec<String> = cmd
        .get_arguments()
        .filter(|a| !a.is_global_set() && !a.is_hide_set() && a.get_long().is_some())
        .map(|a| {
            let long = a.get_long().unwrap_or_default();
            let takes = a
                .get_value_names()
                .map(|v| format!(" <{}>", v.join(" ")))
                .unwrap_or_default();
            format!("--{long}{takes}")
        })
        .collect();
    if !flags.is_empty() {
        s.push_str(&format!(" {}", flags.join(" ")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_lists_all_subcommands() {
        // 用仓内真命令树渲染(经 main 的 Cli::command 不可引,这里以
        // 手工树验渲染器;命令面齐整性由集成测试对着 s5 --llms 验)。
        let cmd = clap::Command::new("s5")
            .subcommand(
                clap::Command::new("put")
                    .about("传")
                    .arg(clap::Arg::new("file").required(true)),
            )
            .subcommand(
                clap::Command::new("tail")
                    .about("读")
                    .arg(clap::Arg::new("stream").required(true)),
            );
        let manual = render_llms(&cmd);
        assert!(
            manual.contains("`s5 put <") && manual.contains("` | 传 |"),
            "用法串含位置参:{manual}"
        );
        assert!(manual.contains("# s5 "), "手册有标题");
        let machine = llm_machine_form(&cmd);
        assert_eq!(machine["commands"].as_array().unwrap().len(), 2);
    }
}
