//! 集成测试:命令面冒烟 + 对真后端的回环。
//!
//! 门控(缺件跳过不失败,证据在回执的测试输出):
//! - 文件道:仓根 `.dev-key.txt`(dev key)+ garage 可达
//! - 流道:`S5_TEST_S2_LITE=1` 且本地 s2-lite 在 `S5_TEST_S2_PORT`(缺省
//!   7171)可达,basin `s5-dev-basin` 已备(`s2 lite` + `s2 create-basin`)

use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use assert_cmd::Command as AssertCommand;
use predicates::prelude::PredicateBooleanExt;

/// 一次测试的环境上下文(每条命令新建,不跨 assert 复用 Command)。
struct Ctx {
    cfg: PathBuf,
    s2_endpoint: Option<String>,
}

impl Ctx {
    fn run(&self) -> AssertCommand {
        let mut c = AssertCommand::cargo_bin("s5").expect("测试目标二进制 s5 必在");
        c.env("S5_CONFIG", &self.cfg);
        if let Some(ep) = &self.s2_endpoint {
            c.env("S5_S2_ENDPOINT", ep)
                .env("S5_S2_BASIN", "s5-dev-basin");
        }
        c
    }
}

fn s5() -> AssertCommand {
    AssertCommand::cargo_bin("s5").expect("测试目标二进制 s5 必在")
}

/// 唯一后缀(测试隔离:同桶/同 basin 并行跑不互踩)。
fn uniq(tag: &str) -> String {
    format!("it/{tag}/{}", std::process::id())
}

/// 临时目录(配置与测试工件共用根)。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("s5-it-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 文件道环境:配置已 init 并写入 dev key;不可达则跳过(None)。
fn s3_ctx(tag: &str) -> Option<Ctx> {
    let key_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".dev-key.txt");
    let raw = std::fs::read_to_string(&key_file).ok()?;
    let key_id = raw.lines().find(|l| l.starts_with("Key ID:"))?;
    let secret = raw.lines().find(|l| l.starts_with("Secret key:"))?;
    let key_id = key_id.trim_start_matches("Key ID:").trim();
    let secret = secret.trim_start_matches("Secret key:").trim();

    let probe = TcpStream::connect_timeout(
        &"192.168.88.2:3900".parse().unwrap(),
        Duration::from_secs(2),
    );
    if probe.is_err() {
        eprintln!("SKIP: garage 192.168.88.2:3900 不可达");
        return None;
    }

    let ctx = Ctx {
        cfg: temp_dir(tag).join("config.toml"),
        s2_endpoint: None,
    };
    let fill = |k: &str, v: &str| {
        ctx.run().args(["config", "set", k, v]).assert().success();
    };
    // init 的 AlreadyExists 防御:本目录唯一 tag,先删旧再 init。
    let _ = std::fs::remove_file(&ctx.cfg);
    ctx.run().args(["config", "init"]).assert().success();
    fill("s3.access_key_id", key_id);
    fill("s3.secret_access_key", secret);
    Some(ctx)
}

/// 流道环境:本地 lite 已起且 basin 已备则启用,否则跳过(None)。
fn s2_ctx(tag: &str) -> Option<Ctx> {
    if std::env::var("S5_TEST_S2_LITE").ok().as_deref() != Some("1") {
        eprintln!("SKIP: S5_TEST_S2_LITE 未设");
        return None;
    }
    let port = std::env::var("S5_TEST_S2_PORT").unwrap_or_else(|_| "7171".into());
    let base = format!("http://127.0.0.1:{port}");
    let probe = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(2),
    );
    if probe.is_err() {
        eprintln!("SKIP: 本地 s2-lite :{port} 未起");
        return None;
    }
    Some(Ctx {
        cfg: temp_dir(tag).join("config.toml"),
        s2_endpoint: Some(base),
    })
}

/// 逐行抽 JSONL 的 body 字段。
fn bodies(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("行恒为 JSON"))
        .map(|v| v["body"].as_str().expect("UTF-8 记录走 body").to_string())
        .collect()
}

#[test]
fn bare_call_prints_help_exit_zero() {
    s5().assert()
        .success()
        .stdout(predicates::str::contains("Usage: s5"));
}

#[test]
fn llms_manual_and_machine_forms() {
    let manual = s5()
        .arg("--llms")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manual = String::from_utf8_lossy(&manual);
    assert!(manual.lines().count() <= 120, "手册至多 120 行(发现面纪律)");
    assert!(manual.contains("退出码"), "手册带退出码节");
    let out = s5()
        .args(["--llms", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("机器形恒为 JSON");
    // 漂移守卫(doc-cli 三面同源):命令集穷尽对锁,新增命令漏手册面即红。
    let mut names: Vec<String> = v["commands"]
        .as_array()
        .expect("commands 是数组")
        .iter()
        .map(|c| c["name"].as_str().expect("名恒在").to_string())
        .collect();
    names.sort();
    let mut expect = [
        "put", "get", "ls", "del", "append", "tail", "streams", "bucket", "config",
    ]
    .map(String::from)
    .to_vec();
    expect.sort();
    assert_eq!(names, expect, "机器形命令集穷尽对锁");
    // 组形子命令与手册面同步对锁(config 与 bucket 两组)。
    let subs = |group: &str| -> Vec<String> {
        v["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == group)
            .and_then(|c| c["subcommands"].as_array())
            .map(|a| {
                a.iter()
                    .map(|s| s["name"].as_str().expect("子命令名恒在").to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        subs("config"),
        ["init", "show", "set", "path"].map(String::from).to_vec(),
        "config 子命令对锁"
    );
    assert_eq!(
        subs("bucket"),
        ["create"].map(String::from).to_vec(),
        "bucket 子命令对锁"
    );
    for name in &expect {
        assert!(
            manual.contains(&format!("s5 {name}")),
            "手册面缺命令 {name}"
        );
    }
}

#[test]
fn json_flag_requires_llms() {
    s5().arg("--json").assert().failure().code(1);
}

/// 帮助与版本展示面 exit 0(agent 帮助面恒成功;mac 冒烟实案补测)。
#[test]
fn help_and_version_exit_zero() {
    s5().arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Usage: s5"));
    s5().arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("s5 "));
    s5().args(["bucket", "create", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("桶名"));
    // 用法错仍退 1(stderr 面,家族形不占 2)。
    s5().arg("--no-such-flag").assert().failure().code(1);
}

#[test]
fn config_face_roundtrip() {
    let ctx = Ctx {
        cfg: temp_dir("cfg-face").join("config.toml"),
        s2_endpoint: None,
    };
    let _ = std::fs::remove_file(&ctx.cfg);
    ctx.run().args(["config", "init"]).assert().success();
    ctx.run()
        .args(["config", "set", "s3.secret_access_key", "supersecret-value"])
        .assert()
        .success();
    let show = ctx.run().args(["config", "show"]).assert().success();
    let out = String::from_utf8_lossy(&show.get_output().stdout).to_string();
    assert!(!out.contains("supersecret-value"), "明文不得回显:{out}");
    ctx.run()
        .args(["config", "init"])
        .assert()
        .failure()
        .code(1);
    let bad = ctx
        .run()
        .args(["config", "set", "s3.oops", "x"])
        .assert()
        .failure()
        .code(1);
    let err = String::from_utf8_lossy(&bad.get_output().stderr);
    assert!(
        err.contains("\"code\":\"Config\""),
        "未知键走 Config 信封:{err}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&ctx.cfg).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "密档必须 0600");
    }
}

#[test]
fn s3_roundtrip_on_dev_bucket() {
    let Some(ctx) = s3_ctx("s3-round") else {
        return;
    };
    let key = format!("{}/artifact.txt", uniq("s3-round"));
    let src = temp_dir("s3-round").join("src.txt");
    std::fs::write(&src, "回环内容 v1\n").unwrap();

    // put → ack 单行 JSON。
    let put = ctx.run().arg("put").arg(&src).arg(&key).assert().success();
    let put_json: serde_json::Value = {
        let o = String::from_utf8_lossy(&put.get_output().stdout);
        serde_json::from_str(o.lines().next().expect("put 出一行 ack")).unwrap()
    };
    assert_eq!(put_json["key"], serde_json::json!(&key));
    assert_eq!(
        put_json["size"],
        serde_json::json!(std::fs::read(&src).unwrap().len() as u64)
    );

    // get stdout 原始字节 → 与源一致。
    let got = ctx.run().arg("get").arg(&key).assert().success();
    assert_eq!(got.get_output().stdout, std::fs::read(&src).unwrap());

    // get -o 落盘。
    let dst = src.with_file_name("dst.txt");
    ctx.run()
        .arg("get")
        .arg(&key)
        .arg("-o")
        .arg(&dst)
        .assert()
        .success();
    assert_eq!(std::fs::read(&dst).unwrap(), std::fs::read(&src).unwrap());

    // ls 前缀命中本键。
    let ls = ctx.run().arg("ls").arg("it/s3-round").assert().success();
    let ls_out = String::from_utf8_lossy(&ls.get_output().stdout);
    assert!(ls_out.contains(&key), "ls 应含本键:{ls_out}");

    // del 后 get 报 NoSuchKey。
    ctx.run().arg("del").arg(&key).assert().success();
    let miss = ctx.run().arg("get").arg(&key).assert().failure().code(1);
    let err = String::from_utf8_lossy(&miss.get_output().stderr);
    assert!(err.contains("NoSuchKey"), "删后取应 NoSuchKey:{err}");
}

#[test]
fn s3_put_from_stdin() {
    let Some(ctx) = s3_ctx("s3-stdin") else {
        return;
    };
    let key = format!("{}/stdin.txt", uniq("s3-stdin"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_s5"))
        .env("S5_CONFIG", &ctx.cfg)
        .args(["put", "-", &key])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("起 s5 put -");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"from stdin\n")
        .expect("注 stdin");
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "stdin put 应成功");
    let got = ctx.run().arg("get").arg(&key).assert().success();
    assert_eq!(got.get_output().stdout, b"from stdin\n");
    ctx.run().arg("del").arg(&key).assert().success();
}

#[test]
fn s2_append_tail_and_streams_on_local_lite() {
    let Some(ctx) = s2_ctx("s2-append") else {
        return;
    };
    let stream = uniq("s2-append");

    // append 三行(流不存在 → 自动建流)。
    let app = ctx
        .run()
        .arg("append")
        .arg(&stream)
        .write_stdin("一\n二\n三\n")
        .assert()
        .success();
    let ack: serde_json::Value = {
        let o = String::from_utf8_lossy(&app.get_output().stdout);
        serde_json::from_str(o.lines().next().expect("ack 单行")).unwrap()
    };
    assert_eq!(ack["start"]["seq_num"], serde_json::json!(0));
    assert_eq!(ack["end"]["seq_num"], serde_json::json!(3), "三行记 0..3");

    // tail 缺省(有界)出全部三条。
    let tail = ctx.run().arg("tail").arg(&stream).assert().success();
    assert_eq!(
        bodies(&String::from_utf8_lossy(&tail.get_output().stdout)),
        ["一", "二", "三"]
    );

    // tail -n 2 只出末两条。
    let t2 = ctx
        .run()
        .arg("tail")
        .arg(&stream)
        .arg("-n")
        .arg("2")
        .assert()
        .success();
    assert_eq!(
        bodies(&String::from_utf8_lossy(&t2.get_output().stdout)),
        ["二", "三"]
    );

    // tail --seq 2 续读(含 2)。
    let ts = ctx
        .run()
        .arg("tail")
        .arg(&stream)
        .arg("--seq")
        .arg("2")
        .assert()
        .success();
    assert_eq!(
        bodies(&String::from_utf8_lossy(&ts.get_output().stdout)),
        ["三"]
    );

    // tail --seq 越尾:游标在尾上,退 0 空出。
    let tb = ctx
        .run()
        .arg("tail")
        .arg(&stream)
        .arg("--seq")
        .arg("99")
        .assert()
        .success();
    assert_eq!(tb.get_output().stdout.len(), 0, "越尾空出");

    // streams 列表含本流(带 `/` 的名字)。
    let ls = ctx
        .run()
        .arg("streams")
        .arg("it/s2-append")
        .assert()
        .success();
    let ls_out = String::from_utf8_lossy(&ls.get_output().stdout);
    assert!(ls_out.contains(&stream), "streams 应含本流:{ls_out}");

    // 不存在的流:信封带 stream_not_found。
    let miss = ctx
        .run()
        .arg("tail")
        .arg(format!("{stream}-nope"))
        .assert()
        .failure()
        .code(1);
    let err = String::from_utf8_lossy(&miss.get_output().stderr);
    assert!(err.contains("stream_not_found"), "缺流信封:{err}");
}

#[test]
fn s2_append_empty_input_no_request() {
    let Some(ctx) = s2_ctx("s2-empty") else {
        return;
    };
    let stream = uniq("s2-empty");
    let out = ctx
        .run()
        .arg("append")
        .arg(&stream)
        .write_stdin("")
        .assert()
        .success();
    assert_eq!(out.get_output().stdout.len(), 0, "空输入不出 stdout");
    // 且未建流(streams 里看不到)。
    let ls = ctx.run().arg("streams").arg(&stream).assert().success();
    let ls_out = String::from_utf8_lossy(&ls.get_output().stdout);
    assert!(!ls_out.contains(&stream), "空输入不得建流:{ls_out}");
}

#[test]
fn s2_tail_follow_runs_until_killed() {
    let Some(ctx) = s2_ctx("s2-follow") else {
        return;
    };
    let stream = uniq("s2-follow");
    ctx.run()
        .arg("append")
        .arg(&stream)
        .write_stdin("hist\n")
        .assert()
        .success();

    let mut child = Command::new(env!("CARGO_BIN_EXE_s5"))
        .env("S5_CONFIG", &ctx.cfg)
        .env(
            "S5_S2_ENDPOINT",
            ctx.s2_endpoint.as_deref().expect("s2 ctx 恒带端点"),
        )
        .env("S5_S2_BASIN", "s5-dev-basin")
        .args(["tail", &stream, "-f"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("起 s5 tail -f");
    std::thread::sleep(Duration::from_secs(2));
    child.kill().expect("杀跟随进程");
    let out = child.wait_with_output().unwrap();
    assert!(
        !out.status.success(),
        "跟随中被杀应非 0 退(SIGINT→130 由手工冒烟与 llms 手册记载)"
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("hist"), "跟随先出历史:{text}");
}

/// 建桶道:admin 道未配(缺 admin_token)即拒,零网络依赖恒跑。
#[test]
fn bucket_create_requires_admin_config() {
    let ctx = Ctx {
        cfg: temp_dir("bkt-nocfg").join("config.toml"),
        s2_endpoint: None,
    };
    let _ = std::fs::remove_file(&ctx.cfg);
    ctx.run().args(["config", "init"]).assert().success();
    // 隔离:门控跑(S5_TEST_GARAGE_ADMIN 面)时剥掉 admin 环境,别让
    // 环境覆盖把"未配"测成"真建桶"。
    ctx.run()
        .env_remove("S5_S3_ADMIN_ENDPOINT")
        .env_remove("S5_S3_ADMIN_TOKEN")
        .args(["bucket", "create", "some-bucket"])
        .assert()
        .failure()
        .stderr(
            predicates::str::contains("\"code\":\"Config\"")
                .and(predicates::str::contains("admin 道未配")),
        );
}

/// 建桶道真回环(garage admin):建桶即授权当前 key,新桶立即可写读。
/// 门控:`S5_TEST_GARAGE_ADMIN=1` 加 `S5_S3_ADMIN_TOKEN`(admin bearer)
/// 加 garage 可达;缺件跳过。收尾尽力删测试桶(admin DELETE)。
#[test]
fn bucket_create_grants_and_roundtrip_on_garage() {
    if std::env::var("S5_TEST_GARAGE_ADMIN").ok().as_deref() != Some("1") {
        eprintln!("SKIP: S5_TEST_GARAGE_ADMIN 未设");
        return;
    }
    let Ok(admin_token) = std::env::var("S5_S3_ADMIN_TOKEN") else {
        eprintln!("SKIP: S5_S3_ADMIN_TOKEN 未设");
        return;
    };
    let Some(mut ctx) = s3_ctx("bkt-live") else {
        eprintln!("SKIP: 文件道前置件缺(dev key 或 garage 不可达)");
        return;
    };
    ctx.s2_endpoint = None;
    let admin_ep =
        std::env::var("S5_S3_ADMIN_ENDPOINT").unwrap_or_else(|_| "http://192.168.88.2:3903".into());
    let name = format!("s5-it-bucket-{}", std::process::id());

    let out = ctx
        .run()
        .env("S5_S3_ADMIN_ENDPOINT", &admin_ep)
        .env("S5_S3_ADMIN_TOKEN", &admin_token)
        .args(["bucket", "create", &name])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ack: serde_json::Value = serde_json::from_slice(&out).expect("建桶 ack 恒 JSON");
    assert_eq!(ack["bucket"], name.as_str(), "ack 带桶名:{ack}");
    assert!(ack["key_granted"].is_string(), "建桶即授权当前 key:{ack}");
    let id = ack["id"].as_str().expect("ack 带 bucket id").to_string();

    // 新桶立即可写读(授权生效面)。
    let put_key = "it/bucket-probe.txt";
    let mut child = Command::new(env!("CARGO_BIN_EXE_s5"))
        .env("S5_CONFIG", &ctx.cfg)
        .env("S5_S3_BUCKET", &name)
        .args(["put", "-", put_key])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("起 s5 put -");
    child
        .stdin
        .as_mut()
        .expect("stdin 恒管道")
        .write_all(b"bucket probe\n")
        .unwrap();
    let put_out = child.wait_with_output().unwrap();
    assert!(put_out.status.success(), "新桶 put 应成功");
    let get_out = ctx
        .run()
        .env("S5_S3_BUCKET", &name)
        .args(["get", put_key])
        .output()
        .expect("起 s5 get");
    assert!(get_out.status.success(), "新桶 get 应成功");
    assert_eq!(String::from_utf8_lossy(&get_out.stdout), "bucket probe\n");

    // 收尾:尽力删测试桶(v1.1.0 无 delete-objects 参数,先清对象再
    // DELETE ?id=;失败不判负,留痕在回执)。
    let _ = Command::new(env!("CARGO_BIN_EXE_s5"))
        .env("S5_CONFIG", &ctx.cfg)
        .env("S5_S3_BUCKET", &name)
        .args(["del", put_key])
        .status();
    let del = format!("{}/v1/bucket?id={id}", admin_ep.trim_end_matches('/'));
    let _ = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("测试清尾 tokio 运行时")
        .block_on(async move {
            reqwest::Client::new()
                .delete(&del)
                .header("authorization", format!("Bearer {admin_token}"))
                .send()
                .await
        });
}
