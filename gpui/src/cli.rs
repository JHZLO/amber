// Amber CLI — 터미널(사람, AI 에이전트)이 켜져 있는 앱을 조작한다. 같은 실행 파일을 명령과 함께 부르면 손님이 되어
// 데이터 폴더의 소켓으로 요청 한 줄(JSON)을 보내고 답 한 줄을 찍는다. 일은 앱이 화면과 같은 길(erd_sync_many)로 한다 —
// 두 프로세스가 한 DB 를 같이 쓰지 않고, 결과가 화면에도 바로 보이게.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use amber_gpui::dbconn::{DbConnection, enabled_schemas};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::*;
use serde_json::{Value, json};

use crate::Spike;

const SOCKET: &str = "amber.sock";

pub const USAGE: &str = "usage:
  amber erd list                          connections and their enabled schemas
  amber erd sync <connection> [schema...] re-read schemas from the database (all enabled if none given)

Amber must be running. Output is JSON; the exit code is 0 when ok, 1 when the command failed, 2 otherwise.";

#[derive(Debug, PartialEq)]
pub enum Call {
    Help,
    Bad,
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

/// 손님 — 명령이면 앱에 묻고 종료 코드를 돌려준다
pub fn client(args: &[String], db_path: &str) -> Option<i32> {
    let req = match parse(args)? {
        Call::Help => {
            println!("{USAGE}");
            return Some(0);
        }
        Call::Bad => {
            eprintln!("{USAGE}");
            return Some(2);
        }
        Call::Send(req) => req,
    };
    let dir = Path::new(db_path).parent().unwrap_or(Path::new("."));
    let Ok(mut stream) = UnixStream::connect(socket_path(dir)) else {
        eprintln!("Amber is not running. Open the app first: open -a Amber");
        return Some(2);
    };
    let mut line = String::new();
    let sent = writeln!(stream, "{req}").and_then(|_| BufReader::new(&stream).read_line(&mut line));
    let out = match sent {
        Ok(_) => serde_json::from_str(&line).unwrap_or_else(|_| fail("no answer from Amber")),
        Err(e) => fail(e.to_string()),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    Some(if out["ok"] == true { 0 } else { 1 })
}

/// 설정에서 복사해 가는 연결 정보 — 이 설치본의 실행 파일 경로로
pub fn setup_text() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "amber".into());
    amber_gpui::i18n::t_with("settings.cli.snippet", &[("cmd", &exe)])
}

type Job = (Value, std::sync::mpsc::Sender<Value>);

/// 앱 — 소켓을 듣는다(실제 앱만, 메인 창이 뜬 뒤). 남은 소켓 파일은 지난 실행의 것이라 지우고 다시 묶는다
pub(crate) fn serve(spike: Entity<Spike>, cx: &mut App) {
    let path = socket_path(&spike.read(cx).db.data_dir());
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else {
        return;
    };
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

fn connections(db: &amber_gpui::db::Db) -> Vec<DbConnection> {
    amber_gpui::dbconn::list_connections(db)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| serde_json::from_value(r).ok())
        .collect()
}

fn local_time(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| {
        d.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

impl Spike {
    pub(crate) fn cli_run(&mut self, req: &Value, cx: &mut Context<Self>) -> Task<Value> {
        match req["cmd"].as_str() {
            Some("erd.list") => Task::ready(self.cli_erd_list()),
            Some("erd.sync") => self.cli_erd_sync(req, cx),
            _ => Task::ready(fail(format!("unknown command: {}", req["cmd"]))),
        }
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
    use super::{Call, parse};
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
    }
}
