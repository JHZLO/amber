// Amber CLI — 터미널(사람, AI 에이전트)이 켜져 있는 앱을 조작한다. 같은 실행 파일을 명령과 함께 부르면 손님이 되어
// 데이터 폴더의 소켓으로 요청 한 줄(JSON)을 보내고 답 한 줄을 찍는다. 일은 앱이 화면과 같은 길(erd_sync_many)로 한다 —
// 두 프로세스가 한 DB 를 같이 쓰지 않고, 결과가 화면에도 바로 보이게.
//
// 보안: 연결한 기기만 부를 수 있다. `amber connect` 가 확인 코드와 함께 요청하면 앱이 앞으로 나와 허용 창을 띄우고,
// 사람이 [허용]을 눌러야 토큰을 돌려준다(2분 안에 안 고르면 거절). 앱은 토큰의 지문(SHA-256)만 남기고, 토큰은 그 기기의
// 키체인에만 있다. 소켓은 0600.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use amber_gpui::db::Db;
use amber_gpui::dbconn::{DbConnection, enabled_schemas};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedSender;
use futures::channel::oneshot;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::Spike;

const SOCKET: &str = "amber.sock";
/// 연결된 기기 목록(지문만) — settings 표
const DEVICES: &str = "amber.cli.devices";
/// 기기 토큰을 두는 키체인 항목. 계정은 데이터 폴더 경로(사본으로 띄운 앱과 섞이지 않게)
const KEYCHAIN_SERVICE: &str = "dev.jhzlo.amber.cli";
const SECURITY_BIN: &str = "/usr/bin/security";
/// 허용 창이 답을 기다리는 시간
pub const PAIR_TTL: Duration = Duration::from_secs(120);
/// 헷갈리는 글자(0 O 1 I)를 뺀 32자 — 바이트 % 32 가 고르다
const CODE_CHARS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub const USAGE: &str = "usage:
  amber connect [name]                    ask Amber to connect this machine (approve it in the window Amber shows)
  amber erd list                          connections and their enabled schemas
  amber erd sync <connection> [schema...] re-read schemas from the database (all enabled if none given)

Amber must be running and this machine connected. Output is JSON; the exit code is 0 when ok,
1 when the command failed, 2 for usage errors, Amber not running, or this machine not connected.";

const NOT_CONNECTED: &str =
    "This machine is not connected. Run `amber connect` and approve the request in Amber.";

#[derive(Debug, PartialEq)]
pub enum Call {
    Help,
    Bad,
    Connect { name: Option<String> },
    Send(Value),
}

/// 인자 → 할 일. None = 명령이 아니다(앱을 띄운다). 앱의 인자(--db 값 등)는 '-' 로 시작하니 첫 낱말로 가른다
pub fn parse(args: &[String]) -> Option<Call> {
    let mut words: Vec<&str> = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--db" {
            it.next();
        } else {
            words.push(a);
        }
    }
    if words.first().is_none_or(|w| w.starts_with('-')) {
        return None;
    }
    Some(match words.as_slice() {
        ["help"] => Call::Help,
        ["connect", name @ ..] => Call::Connect {
            name: (!name.is_empty()).then(|| name.join(" ")),
        },
        ["erd", "list"] => Call::Send(json!({ "cmd": "erd.list" })),
        ["erd", "sync", conn, schemas @ ..] => Call::Send(json!({
            "cmd": "erd.sync",
            "connection": conn,
            "schemas": schemas,
        })),
        _ => Call::Bad,
    })
}

fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOCKET)
}

fn fail(msg: impl Into<String>) -> Value {
    json!({ "ok": false, "error": msg.into() })
}

pub fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "amber".into())
}

// ---- 손님(터미널) ----

fn keychain_token(account: &str) -> Option<String> {
    let out = Command::new(SECURITY_BIN)
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            account,
            "-w",
        ])
        .output()
        .ok()?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !token.is_empty()).then_some(token)
}

/// 셸을 거치지 않는다 — 토큰이 히스토리, 확장에 남지 않는다. -U 는 이미 있으면 바꾼다
fn keychain_save(account: &str, token: &str) -> Result<(), String> {
    let out = Command::new(SECURITY_BIN)
        .args([
            "add-generic-password",
            "-U",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            account,
            "-l",
            "Amber CLI",
            "-w",
            token,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 이 Mac 의 이름 — 설정의 기기 목록에 보인다
fn machine_name() -> String {
    Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "CLI".into())
}

fn ask(dir: &Path, req: &Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path(dir))
        .map_err(|_| "Amber is not running. Open the app first: open -a Amber".to_string())?;
    let mut line = String::new();
    writeln!(stream, "{req}")
        .and_then(|_| BufReader::new(&stream).read_line(&mut line))
        .map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&line).unwrap_or_else(|_| fail("no answer from Amber")))
}

fn print(out: &Value) -> i32 {
    println!("{}", serde_json::to_string_pretty(out).unwrap_or_default());
    if out["ok"] == true { 0 } else { 1 }
}

/// 손님 — 명령이면 앱에 묻고 종료 코드를 돌려준다
pub fn client(args: &[String], db_path: &str) -> Option<i32> {
    let call = parse(args)?;
    let dir = Path::new(db_path).parent().unwrap_or(Path::new("."));
    let account = dir.display().to_string();
    let code = match call {
        Call::Help => {
            println!("{USAGE}");
            0
        }
        Call::Bad => {
            eprintln!("{USAGE}");
            2
        }
        Call::Connect { name } => {
            let name = name.unwrap_or_else(machine_name);
            // 같은 코드가 허용 창에도 뜬다 — 이 터미널의 요청인지 사람이 맞춰 본다
            let code = new_code();
            eprintln!("Approve the request in Amber. Confirmation code: {code}");
            let req = json!({ "cmd": "pair", "code": code, "name": name });
            match ask(dir, &req) {
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
                // 토큰은 키체인으로만 — 화면에 찍지 않는다
                Ok(out) => match out["token"].as_str() {
                    Some(token) => match keychain_save(&account, token) {
                        Ok(()) => print(&json!({ "ok": true, "name": out["name"] })),
                        Err(e) => print(&fail(format!(
                            "couldn't save the token to the Keychain: {e}"
                        ))),
                    },
                    None => print(&out),
                },
            }
        }
        Call::Send(mut req) => {
            let Some(token) = keychain_token(&account) else {
                eprintln!("{NOT_CONNECTED}");
                return Some(2);
            };
            req["token"] = json!(token);
            match ask(dir, &req) {
                Ok(out) => print(&out),
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
            }
        }
    };
    Some(code)
}

/// 설정에서 복사해 가는 명령 안내 — 이 설치본의 실행 파일 경로로
pub fn setup_text() -> String {
    amber_gpui::i18n::t_with("settings.cli.snippet", &[("cmd", &exe_path())])
}

// ---- 연결된 기기(앱) ----

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    hash: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

pub fn devices(db: &Db) -> Vec<Device> {
    amber_gpui::prefs::get(db, DEVICES)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_devices(db: &Db, list: &[Device]) {
    amber_gpui::prefs::set(
        db,
        DEVICES,
        &serde_json::to_string(list).unwrap_or_default(),
    );
}

pub fn revoke(db: &Db, id: &str) {
    let mut list = devices(db);
    list.retain(|d| d.id != id);
    save_devices(db, &list);
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

fn random<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .expect("/dev/urandom");
    buf
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// 길이가 같으면 끝까지 비교한다 — 어디서 틀렸는지 시간으로 새지 않게
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 대시, 공백, 대소문자는 가리지 않는다
fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// 확인 코드 — 터미널과 허용 창에 같은 꼴(XXXX-XXXX)로 뜬다
pub fn new_code() -> String {
    let raw: String = random::<8>()
        .iter()
        .map(|b| CODE_CHARS[(*b % 32) as usize] as char)
        .collect();
    format!("{}-{}", &raw[..4], &raw[4..])
}

/// 허용 창이 떠 있는 연결 요청 — 사람이 고르면 reply 로 답이 간다
pub struct PairRequest {
    pub name: String,
    pub code: String,
    reply: Option<oneshot::Sender<Value>>,
}

// ---- 앱 ----

type Job = (Value, std::sync::mpsc::Sender<Value>);

/// 앱 — 소켓을 듣는다(실제 앱만, 메인 창이 뜬 뒤). 남은 소켓 파일은 지난 실행의 것이라 지우고 다시 묶는다
pub(crate) fn serve(main_window: AnyWindowHandle, spike: Entity<Spike>, cx: &mut App) {
    let path = socket_path(&spike.read(cx).db.data_dir());
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else {
        return;
    };
    // 나만 — 다른 사용자는 연결조차 못 한다
    if std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).is_err() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<Job>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || answer(stream, &tx));
        }
    });
    let weak = spike.downgrade();
    cx.spawn(async move |cx| {
        while let Some((req, reply)) = rx.next().await {
            // 연결 요청이면 허용 창이 보이게 — 트레이로 숨겨 둔 창도 앞으로
            if req["cmd"] == "pair" {
                let _ = cx.update(|cx| crate::shell::show_main(main_window, cx));
            }
            let Ok(task) = weak.update(cx, |s, cx| s.cli_run(&req, cx)) else {
                return;
            };
            // 동기화는 오래 걸린다 — 기다리는 사이 다른 요청도 받는다
            cx.spawn(async move |_| {
                let _ = reply.send(task.await);
            })
            .detach();
        }
    })
    .detach();
}

/// 연결 하나 — 요청 한 줄을 앱에 넘기고 답 한 줄을 돌려준다
fn answer(stream: UnixStream, tx: &UnboundedSender<Job>) {
    let mut line = String::new();
    let out = match BufReader::new(&stream).read_line(&mut line) {
        Ok(_) => match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                let (reply, wait) = std::sync::mpsc::channel();
                if tx.unbounded_send((req, reply)).is_ok() {
                    wait.recv()
                        .unwrap_or_else(|_| fail("Amber dropped the request"))
                } else {
                    fail("Amber is closing")
                }
            }
            Err(e) => fail(e.to_string()),
        },
        Err(e) => fail(e.to_string()),
    };
    let _ = writeln!(&stream, "{out}");
}

fn connections(db: &Db) -> Vec<DbConnection> {
    amber_gpui::dbconn::list_connections(db)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| serde_json::from_value(r).ok())
        .collect()
}

pub fn local_time(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| {
        d.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

impl Spike {
    pub(crate) fn cli_run(&mut self, req: &Value, cx: &mut Context<Self>) -> Task<Value> {
        if req["cmd"] == "pair" {
            return self.cli_request_pair(req, cx);
        }
        if !self.cli_authorized(req, cx) {
            return Task::ready(fail(NOT_CONNECTED));
        }
        match req["cmd"].as_str() {
            Some("erd.list") => Task::ready(self.cli_erd_list()),
            Some("erd.sync") => self.cli_erd_sync(req, cx),
            _ => Task::ready(fail(format!("unknown command: {}", req["cmd"]))),
        }
    }

    /// 연결 요청 — 허용 창을 띄우고 사람이 고를 때까지(최대 2분) 기다린다. 한 번에 하나만
    fn cli_request_pair(&mut self, req: &Value, cx: &mut Context<Self>) -> Task<Value> {
        if self.settings.cli_request.is_some() {
            return Task::ready(fail("Another connection request is waiting in Amber."));
        }
        let raw = normalize(req["code"].as_str().unwrap_or_default());
        if raw.len() != 8 || !raw.bytes().all(|b| CODE_CHARS.contains(&b)) {
            return Task::ready(fail("bad confirmation code"));
        }
        let code = format!("{}-{}", &raw[..4], &raw[4..]);
        let name: String = req["name"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("CLI")
            .chars()
            .take(60)
            .collect();
        let (tx, rx) = oneshot::channel();
        self.settings.cli_request = Some(PairRequest {
            name,
            code: code.clone(),
            reply: Some(tx),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let timer = Box::pin(cx.background_executor().timer(PAIR_TTL));
            match futures::future::select(rx, timer).await {
                futures::future::Either::Left((Ok(out), _)) => out,
                futures::future::Either::Left((Err(_), _)) => fail("Denied in Amber."),
                futures::future::Either::Right(_) => {
                    let _ = this.update(cx, |s, cx| {
                        if s.settings
                            .cli_request
                            .as_ref()
                            .is_some_and(|r| r.code == code)
                        {
                            s.settings.cli_request = None;
                            cx.notify();
                        }
                    });
                    fail("Not approved within 2 minutes.")
                }
            }
        })
    }

    /// 허용 창의 [허용]/[거절] — 허용이면 새 토큰을 한 번 돌려주고 지문만 남긴다
    pub(crate) fn cli_decide(&mut self, allow: bool, cx: &mut Context<Self>) {
        let Some(mut r) = self.settings.cli_request.take() else {
            return;
        };
        let out = if allow {
            let token = hex(&random::<32>());
            let mut list = devices(&self.db);
            list.push(Device {
                id: ulid::Ulid::new().to_string(),
                name: r.name.clone(),
                hash: digest(&token),
                created_at: now_ms(),
                last_used: None,
            });
            save_devices(&self.db, &list);
            json!({ "ok": true, "token": token, "name": r.name })
        } else {
            fail("Denied in Amber.")
        };
        if let Some(tx) = r.reply.take() {
            let _ = tx.send(out);
        }
        cx.notify();
    }

    /// 토큰의 지문이 연결된 기기 중 하나와 같은가 — 맞으면 마지막 사용을 남긴다
    fn cli_authorized(&mut self, req: &Value, cx: &mut Context<Self>) -> bool {
        let Some(token) = req["token"].as_str() else {
            return false;
        };
        let hash = digest(token);
        let mut list = devices(&self.db);
        let Some(d) = list
            .iter_mut()
            .find(|d| same(d.hash.as_bytes(), hash.as_bytes()))
        else {
            return false;
        };
        d.last_used = Some(now_ms());
        save_devices(&self.db, &list);
        cx.notify();
        true
    }

    fn cli_erd_list(&self) -> Value {
        let list: Vec<Value> = connections(&self.db)
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "env": c.env,
                    "kind": c.kind,
                    "schemas": enabled_schemas(c).iter().map(|p| &p.name).collect::<Vec<_>>(),
                    "last_sync": c.last_sync_at.and_then(local_time),
                    "last_error": c.last_error,
                })
            })
            .collect();
        json!({ "ok": true, "connections": list })
    }

    /// 화면의 [모든 스키마 동기화]와 같은 길 — 스키마를 주면 그것만(켜 둔 스키마여야 한다)
    fn cli_erd_sync(&mut self, req: &Value, cx: &mut Context<Self>) -> Task<Value> {
        let name = req["connection"].as_str().unwrap_or_default().to_string();
        let Some(c) = connections(&self.db)
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
        else {
            return Task::ready(fail(format!("no connection named {name:?}")));
        };
        let enabled: Vec<String> = enabled_schemas(&c).iter().map(|p| p.name.clone()).collect();
        let asked: Vec<String> = req["schemas"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(s) = asked.iter().find(|s| !enabled.contains(*s)) {
            return Task::ready(fail(format!(
                "{s:?} is not an enabled schema of {:?}",
                c.name
            )));
        }
        let todo = if asked.is_empty() { enabled } else { asked };
        let task = self.erd_sync_many(&c, todo.clone(), cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let synced = task.await;
            let error = this.update(cx, |s, _| s.erd.error.clone()).ok().flatten();
            let missed: Vec<&String> = todo.iter().filter(|s| !synced.contains(*s)).collect();
            json!({
                "ok": missed.is_empty(),
                "connection": c.name,
                "synced": synced,
                "not_synced": missed,
                "error": error,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    // super::* 는 gpui_kit 의 test 매크로까지 끌어와 #[test] 를 가린다
    use super::{CODE_CHARS, Call, new_code, normalize, parse, same};
    use serde_json::json;

    fn args(s: &str) -> Vec<String> {
        std::iter::once("amber")
            .chain(s.split_whitespace())
            .map(String::from)
            .collect()
    }

    #[test]
    fn app_flags_still_open_the_app() {
        assert_eq!(parse(&args("")), None);
        assert_eq!(parse(&args("--db /tmp/x/amber.db")), None);
        assert_eq!(parse(&args("--note a.md --date 2026-11-27")), None);
    }

    #[test]
    fn commands_become_requests() {
        assert_eq!(parse(&args("help")), Some(Call::Help));
        assert_eq!(parse(&args("erd")), Some(Call::Bad));
        assert_eq!(
            parse(&args("erd list --db /tmp/x/amber.db")),
            Some(Call::Send(json!({ "cmd": "erd.list" })))
        );
        assert_eq!(
            parse(&args("erd sync MySQL trp_settlement")),
            Some(Call::Send(json!({
                "cmd": "erd.sync",
                "connection": "MySQL",
                "schemas": ["trp_settlement"],
            })))
        );
        assert_eq!(
            parse(&args("connect work laptop")),
            Some(Call::Connect {
                name: Some("work laptop".into()),
            })
        );
        assert_eq!(parse(&args("connect")), Some(Call::Connect { name: None }));
    }

    #[test]
    fn confirmation_codes_are_readable_and_tokens_compared_whole() {
        let code = new_code();
        assert_eq!(code.len(), 9);
        assert_eq!(&code[4..5], "-");
        assert!(
            code.bytes()
                .filter(|b| *b != b'-')
                .all(|b| CODE_CHARS.contains(&b))
        );
        assert_eq!(normalize(" ab12-cd34 "), "AB12CD34");
        assert!(same(b"AB12CD34", b"AB12CD34"));
        assert!(!same(b"AB12CD34", b"AB12CD35"));
        assert!(!same(b"AB12", b"AB12CD34"));
    }
}
