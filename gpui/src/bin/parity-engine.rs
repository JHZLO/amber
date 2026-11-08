// 동등성 하네스(parity/engine.ts)의 외부 엔진. 표준 입출력으로 한 줄에 JSON 하나씩.
//   AMBER_PARITY_ENGINE="gpui/target/debug/parity-engine" pnpm parity

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use amber_gpui::db::{Clock, Db};
use serde_json::{Value as Json, json};

fn main() {
    // 하네스의 TS 엔진은 node 에서 돌아 언어가 ko 다(i18n.getLang)
    amber_gpui::i18n::init(Some("ko"));
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
            "ops" => json!({"ok": true, "ops": amber_gpui::OPS, "migrate": true}),
            "open" => match {
                // "오늘"(리포트 digest 등)은 하네스가 정한 시간대로 센다. 엔진은 한 줄씩 도는 단일 스레드다
                if let Some(tz) = msg["tz"].as_str() {
                    unsafe { std::env::set_var("TZ", tz) };
                }
                Db::open(
                    msg["db"].as_str().unwrap_or_default(),
                    Clock::Fixed(now.clone()),
                )
            } {
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
            // 빈 파일에 마이그레이션을 깐다 — 하네스가 장부와 스키마를 지금 앱의 것과 견준다
            "migrate" => match Db::open(
                msg["db"].as_str().unwrap_or_default(),
                Clock::Fixed(now.clone()),
            )
            .map_err(|e| e.to_string())
            .and_then(|d| amber_gpui::migrate::run(&d))
            {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"ok": false, "error": e}),
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
