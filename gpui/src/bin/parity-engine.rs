// 동등성 하네스(parity/engine.ts)의 외부 엔진. 표준 입출력으로 한 줄에 JSON 하나씩.
//   AMBER_PARITY_ENGINE="gpui/target/debug/parity-engine" pnpm parity

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use amber_gpui::db::{Clock, Db};
use serde_json::{Value as Json, json};

fn main() {
    let now = Arc::new(AtomicI64::new(0));
    let mut db: Option<Db> = None;
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let msg: Json = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                reply(&stdout, json!({"ok": false, "error": e.to_string()}));
                continue;
            }
        };
        let out = match msg["cmd"].as_str().unwrap_or_default() {
            "ops" => json!({"ok": true, "ops": amber_gpui::OPS, "migrate": false}),
            "open" => match Db::open(
                msg["db"].as_str().unwrap_or_default(),
                Clock::Fixed(now.clone()),
            ) {
                Ok(d) => {
                    db = Some(d);
                    json!({"ok": true})
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            },
            "op" => match &db {
                Some(d) => {
                    now.store(msg["now"].as_i64().unwrap_or_default(), Ordering::SeqCst);
                    let args = msg["args"].as_array().cloned().unwrap_or_default();
                    match amber_gpui::call(d, msg["op"].as_str().unwrap_or_default(), &args) {
                        Ok(result) => json!({"ok": true, "result": result}),
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                }
                None => json!({"ok": false, "error": "열린 DB 가 없다"}),
            },
            "close" => {
                db = None;
                json!({"ok": true})
            }
            other => json!({"ok": false, "error": format!("모르는 cmd: {other}")}),
        };
        reply(&stdout, out);
    }
}

fn reply(out: &std::io::Stdout, msg: Json) {
    let mut out = out.lock();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}
