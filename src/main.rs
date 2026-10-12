//! s5 CLI 入口:文件道与流道子命令分发、`--llms` 手册面与错误信封出口。
//!
//! 裸 `s5` 打印帮助退 0(agent 裸调用面恒成功);`--help`/`--version`
//! 展示面 stdout 退 0;错误 stderr 单行 JSON
//! {code, message} 退 1(解析级用法错同通道,不占 2);`tail -f` SIGINT
//! 退 130。

use std::io::{BufRead, Write};

use clap::{CommandFactory, Parser, Subcommand};
use s5::config::{Config, ConfigError};
use s5::{bucketchan, filechan, llms, streamchan};

#[derive(Parser)]
#[command(
    name = "s5",
    version,
    about = "S5(Shared Store & Streams):agent 跨机共享 CLI(S3 文件道 put/get/ls/del 加 bucket create 建桶,流道 append/tail;全开放共享)",
    help_template = "{name}@{version} {about}\n\n{usage-heading} {usage}\n\n{all-args}{after-help}",
    after_help = "agent 手册面:s5 --llms(markdown 手册);s5 --llms --json(机器形)"
)]
struct Cli {
    /// 打印 agent 说明书(markdown 手册;配 --json 出机器形)后退出
    #[arg(long)]
    llms: bool,
    /// 与 --llms 合用出机器形手册(其余命令输出本就是行式 JSON/JSONL)
    #[arg(long, requires = "llms")]
    json: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 上传本地文件到远端键(S3 单次 Put;源 `-` 表 stdin 整读)
    Put {
        /// 本地文件路径或 `-`(stdin)
        file: String,
        /// 远端键(字面键名,非 glob;不以 / 结尾)
        key: String,
    },
    /// 取回对象为原始字节(缺省 stdout;`-o` 落文件)
    Get {
        /// 远端键
        key: String,
        /// 落盘目标路径(缺省 stdout 出原始字节)
        #[arg(short, long)]
        out: Option<String>,
    },
    /// 前缀列举对象(字面前缀非 glob;空前缀全列;键序升序,JSONL)
    Ls {
        /// 键前缀(可空)
        prefix: Option<String>,
    },
    /// 删除对象(S3 幂等语义:键不存在同样报成功)
    Del {
        /// 远端键
        key: String,
    },
    /// 逐行成记录追加到流(缺省 stdin;流不存在自动建;输出单行 AppendAck)
    Append {
        /// 流名(可含 `/`,那是名字字符不是目录)
        stream: String,
        /// 输入文件路径,`-` 或缺省表 stdin;空输入不发请求退 0
        #[arg(short, long, default_value = "-")]
        input: String,
    },
    /// 读流尾(有界退出;`-f` 跟随到 SIGINT 退 130;JSONL 逐行)
    Tail {
        /// 流名
        stream: String,
        /// 最近 N 条(缺省 20;单批上限 1000,超出按 1000 记)
        #[arg(short, long, default_value_t = 20)]
        n: u64,
        /// 从该序号(含)续读;与 -n 互斥使用时以本参定起点
        #[arg(long)]
        seq: Option<u64>,
        /// 先出历史再跟随新记录直到 SIGINT
        #[arg(short, long)]
        follow: bool,
    },
    /// 列配置 basin 下的流(可选字面前缀;JSONL)
    Streams {
        /// 流名前缀(可空)
        prefix: Option<String>,
    },
    /// 建桶(garage admin 道:全开放共享,建桶即授权当前 key 读写)
    Bucket {
        #[command(subcommand)]
        cmd: BucketCmd,
    },
    /// 配置层(零感知凭据的落点:0600 密档 + 环境变量覆盖)
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
}

#[derive(Subcommand)]
enum BucketCmd {
    /// 建全局名桶并授权配置层当前 s3 key 读写(重名报 BucketAlreadyExists)
    Create {
        /// 桶名(全局别名,非路径)
        name: String,
    },
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// 落 0600 缺省配置(已存在则报错不覆盖)
    Init,
    /// 展示生效配置(密钥类掩码,永不回显明文)
    Show,
    /// 设一个配置键(点径:s3.endpoint/s3.region/s3.bucket/s3.access_key_id/s3.secret_access_key/s3.admin_endpoint/s3.admin_token/s2.endpoint/s2.basin/s2.token)
    Set {
        /// 点径键
        key: String,
        /// 新值
        value: String,
    },
    /// 打印配置文件路径
    Path,
}

fn main() {
    // 解析级分流:clap 展示类(--help/--version)stdout 退 0(agent 帮助
    // 面恒成功);用法错 stderr 退 1(与业务错同通道,不占 2;家族形
    // hst REQ-032)。
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            std::process::exit(i32::from(e.use_stderr()));
        }
    };
    if cli.llms {
        if cli.json {
            println!("{}", llms::llm_machine_form(&Cli::command()));
        } else {
            print!("{}", llms::render_llms(&Cli::command()));
        }
        return;
    }
    let Some(cmd) = cli.command else {
        // 裸调用面:打印帮助退 0(不弹交互)。
        let mut stdout = std::io::stdout().lock();
        let _ = write!(stdout, "{}", Cli::command().render_help());
        return;
    };
    if let Err(code) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio 运行时构建失败属装机问题")
        .block_on(dispatch(cmd))
    {
        std::process::exit(code);
    }
}

/// 错误信封出口:stderr 单行 JSON {code, message},退 1。
fn fail(code: &str, message: impl std::fmt::Display) -> i32 {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(
        stderr,
        "{}",
        serde_json::json!({ "code": code, "message": message.to_string() })
    );
    1
}

async fn dispatch(cmd: Commands) -> Result<(), i32> {
    match cmd {
        Commands::Config { cmd } => dispatch_config(cmd),
        Commands::Put { file, key } => {
            let cfg = load_cfg()?;
            match filechan::put(&cfg, &file, &key).await {
                Ok(out) => {
                    println!(
                        "{}",
                        serde_json::to_string(&out).expect("PutOutcome 恒可序列化")
                    );
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Get { key, out } => {
            let cfg = load_cfg()?;
            match filechan::get(&cfg, &key).await {
                Ok(bytes) => {
                    match out {
                        Some(path) => {
                            std::fs::write(&path, &bytes)
                                .map_err(|e| fail("LocalIo", format!("写 {path} 失败: {e}")))?;
                            println!(
                                "{}",
                                serde_json::json!({ "key": key, "bytes": bytes.len(), "path": path })
                            );
                        }
                        None => {
                            let mut stdout = std::io::stdout().lock();
                            stdout
                                .write_all(&bytes)
                                .map_err(|e| fail("LocalIo", format!("写 stdout 失败: {e}")))?;
                            stdout
                                .flush()
                                .map_err(|e| fail("LocalIo", format!("flush stdout 失败: {e}")))?;
                        }
                    }
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Ls { prefix } => {
            let cfg = load_cfg()?;
            match filechan::ls(&cfg, prefix.as_deref().unwrap_or("")).await {
                Ok(rows) => {
                    let mut stdout = std::io::stdout().lock();
                    for row in rows {
                        let _ = writeln!(
                            stdout,
                            "{}",
                            serde_json::to_string(&row).expect("ObjectRow 恒可序列化")
                        );
                    }
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Del { key } => {
            let cfg = load_cfg()?;
            match filechan::del(&cfg, &key).await {
                Ok(k) => {
                    println!("{}", serde_json::json!({ "key": k, "deleted": true }));
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Append { stream, input } => {
            let cfg = load_cfg()?;
            let lines = read_lines(&input).map_err(|e| fail(e.0, e.1))?;
            match streamchan::append(&cfg, &stream, lines).await {
                Ok(Some(ack)) => {
                    println!(
                        "{}",
                        serde_json::to_string(&ack).expect("AppendOutcome 恒可序列化")
                    );
                    Ok(())
                }
                Ok(None) => {
                    eprintln!(
                        "{}",
                        serde_json::json!({ "code": "EmptyInput", "message": "无输入记录,未发追加请求" })
                    );
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Tail {
            stream,
            n,
            seq,
            follow,
        } => {
            let cfg = load_cfg()?;
            let n = n.clamp(1, 1000);
            if follow {
                let stdout = std::io::stdout();
                let mut w = stdout.lock();
                let res = streamchan::follow(&cfg, &stream, n, seq, |row| {
                    let _ = writeln!(
                        w,
                        "{}",
                        serde_json::to_string(row).expect("RecordRow 恒可序列化")
                    );
                    let _ = w.flush();
                })
                .await;
                match res {
                    // SIGINT 收束退 130(shell 惯例),与业务错 1 分通道。
                    Ok(streamchan::FollowEnd::Interrupted) => Err(130),
                    Err(e) => {
                        let code = e.code().to_string();
                        let msg = e.message();
                        Err(fail(&code, msg))
                    }
                }
            } else {
                match streamchan::tail(&cfg, &stream, n, seq).await {
                    Ok(rows) => {
                        let mut stdout = std::io::stdout().lock();
                        for row in rows {
                            let _ = writeln!(
                                stdout,
                                "{}",
                                serde_json::to_string(&row).expect("RecordRow 恒可序列化")
                            );
                        }
                        Ok(())
                    }
                    Err(e) => {
                        let code = e.code().to_string();
                        let msg = e.message();
                        Err(fail(&code, msg))
                    }
                }
            }
        }
        Commands::Streams { prefix } => {
            let cfg = load_cfg()?;
            match streamchan::list_streams(&cfg, prefix.as_deref()).await {
                Ok(rows) => {
                    let mut stdout = std::io::stdout().lock();
                    for row in rows {
                        let _ = writeln!(
                            stdout,
                            "{}",
                            serde_json::to_string(&row).expect("StreamRow 恒可序列化")
                        );
                    }
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
        Commands::Bucket { cmd } => {
            let BucketCmd::Create { name } = cmd;
            let cfg = load_cfg()?;
            match bucketchan::create(&cfg, &name).await {
                Ok(out) => {
                    println!(
                        "{}",
                        serde_json::to_string(&out).expect("建桶 ack 恒可序列化")
                    );
                    Ok(())
                }
                Err(e) => {
                    let code = e.code().to_string();
                    let msg = e.message();
                    Err(fail(&code, msg))
                }
            }
        }
    }
}

fn dispatch_config(cmd: ConfigCmd) -> Result<(), i32> {
    match cmd {
        ConfigCmd::Init => {
            let path = Config::path();
            if path.exists() {
                return Err(fail(
                    "AlreadyExists",
                    format!("配置已存在 {}(改值用 s5 config set)", path.display()),
                ));
            }
            Config::default().save().map_err(cfg_fail)?;
            println!("{}", serde_json::json!({ "path": path }));
            Ok(())
        }
        ConfigCmd::Show => {
            let cfg = load_cfg()?;
            println!("{}", cfg.masked());
            Ok(())
        }
        ConfigCmd::Set { key, value } => {
            let mut cfg = Config::load().map_err(cfg_fail)?;
            cfg.set(&key, &value).map_err(cfg_fail)?;
            cfg.save().map_err(cfg_fail)?;
            println!("{}", serde_json::json!({ "key": key, "updated": true }));
            Ok(())
        }
        ConfigCmd::Path => {
            println!("{}", serde_json::json!({ "path": Config::path() }));
            Ok(())
        }
    }
}

fn load_cfg() -> Result<Config, i32> {
    Config::load().map_err(cfg_fail)
}

fn cfg_fail(e: ConfigError) -> i32 {
    fail("Config", e)
}

/// 逐行读输入(`-` 或缺省 stdin);非 UTF-8 行如实报错(记录面按 UTF-8 设计)。
fn read_lines(input: &str) -> Result<Vec<String>, (&'static str, String)> {
    let reader: Box<dyn BufRead> = if input == "-" {
        Box::new(std::io::stdin().lock())
    } else {
        Box::new(std::io::BufReader::new(
            std::fs::File::open(input).map_err(|e| ("LocalIo", format!("开 {input} 失败: {e}")))?,
        ))
    };
    let mut lines = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| ("LocalIo", format!("读 {input} 第 {} 行失败: {e}", i + 1)))?;
        lines.push(line);
    }
    Ok(lines)
}
