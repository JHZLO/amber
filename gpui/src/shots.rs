// 화면 찍기 — 손으로 못 하는 조작(hover, 끌기 도중)을 실제 macOS 렌더링으로 찍어 PNG 로 남긴다.
// 창은 화면 밖에 뜨고 마우스는 이벤트로 보낸다. 데이터는 데모다(실제 할 일을 찍지 않는다).
//   cargo run --features shots -- --shots <폴더>

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use amber_gpui::db::{Clock, Db};
use amber_gpui::todos;
use gpui_kit::*;

use crate::{Spike, assets, theme};

const DATE: &str = "2026-11-05";

const DEMO_CONCEPTS: &str = r#"INSERT INTO concepts (ulid, title, summary, detail_path, status, confidence, created_at, updated_at) VALUES
 ('01JDEMO0000000000000000001', 'Write-ahead log', 'Append each change to a log before applying it, so a crash can replay it.', 'concepts/01JDEMO0000000000000000001/index.md', 'learning', 2, 1762300000000, 1762390000000),
 ('01JDEMO0000000000000000002', 'Idempotency key', 'Send the same request twice and get one effect.', 'concepts/01JDEMO0000000000000000002/index.md', 'learning', 1, 1762310000000, 1762320000000),
 ('01JDEMO0000000000000000003', 'Backpressure', 'Slow the producer down when the consumer falls behind.', 'concepts/01JDEMO0000000000000000003/index.md', 'learned', 3, 1762200000000, 1762380000000),
 ('01JDEMO0000000000000000004', 'Two-phase commit', 'Prepare on every participant, then commit on every participant.', 'concepts/01JDEMO0000000000000000004/index.md', 'learning', 1, 1762330000000, 1762340000000),
 ('01JDEMO0000000000000000005', 'MVCC', 'Readers see a snapshot while writers create new row versions.', 'concepts/01JDEMO0000000000000000005/index.md', 'learned', 3, 1762100000000, 1762370000000);
INSERT INTO tags (name) VALUES ('db'), ('storage'), ('api'), ('kafka'), ('distributed');
INSERT INTO concept_tags (concept_id, tag_id)
 SELECT c.id, t.id FROM concepts c, tags t WHERE
  (c.title = 'Write-ahead log' AND t.name IN ('db','storage')) OR
  (c.title = 'Idempotency key' AND t.name = 'api') OR
  (c.title = 'Backpressure' AND t.name = 'kafka') OR
  (c.title = 'Two-phase commit' AND t.name IN ('db','distributed')) OR
  (c.title = 'MVCC' AND t.name = 'db');
"#;

fn seed(dir: &Path) -> String {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("amber.db");
    // 보관함 — 폴더 둘, 노트 셋
    let notes = dir.join("vault/notes");
    std::fs::create_dir_all(notes.join("Engineering/Runbooks")).unwrap();
    std::fs::write(notes.join("Inbox.md"), "# Inbox\n\n- Read the GPUI docs\n").unwrap();
    std::fs::create_dir_all(notes.join("_assets")).unwrap();
    std::fs::write(
        notes.join("_assets/logo.png"),
        include_bytes!("../../src-tauri/icons/128x128.png"),
    )
    .unwrap();
    // 읽기 충실도 견본 — 지금 앱(웹)과 나란히 찍어 맞춘다
    std::fs::write(
        notes.join("Reading.md"),
        include_str!("../assets/reading-sample.md"),
    )
    .unwrap();
    // 다이어그램 보관함 — 폴더 하나, 파일 둘
    let diagrams = dir.join("vault/diagrams");
    std::fs::create_dir_all(diagrams.join("ERD")).unwrap();
    std::fs::write(
        diagrams.join("ERD/Orders.mmd"),
        "erDiagram\n  USER ||--o{ ORDER : places\n  ORDER ||--|{ ORDER_ITEM : contains\n  USER {\n    bigint id PK\n    varchar email\n  }\n  ORDER {\n    bigint id PK\n    bigint user_id FK\n  }\n",
    )
    .unwrap();
    std::fs::write(
        diagrams.join("Release flow.mmd"),
        "flowchart LR\n  A[Tag] --> B[Push] --> C[Rebuild app]\n",
    )
    .unwrap();
    std::fs::write(
        notes.join("Engineering/Runbooks/Release.md"),
        "# Release\n\n1. Tag\n2. Push\n",
    )
    .unwrap();
    std::fs::write(
        notes.join("Engineering/Parity harness.md"),
        "## 1. Why\n\nThe harness records every step of the **TypeScript** engine and replays it \
against the Rust one. See [the steps](#2).\n\n> [!NOTE]\n> Fixtures live in `parity/fixtures`.\n\n\
## 2. Steps\n\n- Record with `pnpm parity:record`\n- Replay with `pnpm parity`\n\n\
```mermaid\nflowchart LR\n  TS[TS engine] --> F[(fixtures)]\n  F --> RS[Rust engine]\n```\n",
    )
    .unwrap();
    // 인라인 질문 둘 — 하나는 이어 물은 스레드
    let now = chrono::Local::now().timestamp_millis();
    std::fs::write(
        notes.join("Engineering/Parity harness.comments.json"),
        serde_json::to_string_pretty(&serde_json::json!({ "version": 1, "comments": [
            { "id": "c1", "anchor": "records every step", "occurrence": 0,
              "question": "Does it record the DB too?",
              "answer": "Yes. Each step stores the return value **and** the DB diff, so a replay compares both.",
              "createdAt": now - 2 * 3_600_000, "model": "sonnet" },
            { "id": "c2", "anchor": "Replay with", "occurrence": 0,
              "question": "What engine runs on replay?",
              "answer": "The Rust engine, through `AMBER_PARITY_ENGINE`.",
              "createdAt": now - 26 * 3_600_000, "model": "sonnet",
              "followUps": [{ "question": "And without it?",
                "answer": "The reference engine replays the fixtures, so the harness checks itself.",
                "createdAt": now - 25 * 3_600_000, "model": "sonnet" }] }
        ]}))
        .unwrap(),
    )
    .unwrap();
    let db = Db::open(db_path.to_str().unwrap(), Clock::System).unwrap();
    amber_gpui::migrate::run(&db).unwrap();
    amber_gpui::ai_config::skip_ai_onboarding(&db).unwrap();
    // AI 연결(실행은 안 한다 — 찍기는 스토어 상태를 직접 놓는다), 저장 프롬프트 하나, 최근 참고 폴더 둘
    amber_gpui::ai_config::connect_provider(&db, "claude", "/usr/bin/false", None).unwrap();
    amber_gpui::prefs::set(
        &db,
        "note_ai_prompts",
        r#"[{"id":"p1","label":"Team style","text":"Short sentences, no brand names."}]"#,
    );
    amber_gpui::prefs::set(
        &db,
        "amber.ai.refDirs",
        r#"["/Users/me/src/amber","/Users/me/src/resin"]"#,
    );
    // 묶음 하나(하위 셋) + 낱개 셋
    // 개념 다섯(브라우저 하네스 사본에도 같은 것을 넣어 지금 앱과 견준다)
    for stmt in DEMO_CONCEPTS.split(';').filter(|q| !q.trim().is_empty()) {
        db.execute(stmt, &[]).unwrap();
    }
    let wal = dir.join("vault/concepts/01JDEMO0000000000000000001");
    std::fs::create_dir_all(&wal).unwrap();
    std::fs::write(
        wal.join("index.md"),
        "# Write-ahead log\n\nEvery change is appended to a log on disk **before** the data pages change.\n\n## Why\n\n- Crash recovery replays the log\n- Writes become sequential\n",
    )
    .unwrap();
    let g = todos::create_todo(&db, "Release v0.24", DATE, None, "day").unwrap();
    for c in ["Write the changelog", "Tag and push", "Rebuild the app"] {
        todos::create_todo(&db, c, DATE, Some(g), "day").unwrap();
    }
    for c in [
        "Review the parity fixtures",
        "Reply to the design thread",
        "Book the meeting room",
    ] {
        todos::create_todo(&db, c, DATE, None, "day").unwrap();
    }
    for (s, e, title) in [
        (540, 570, "Standup"),
        (600, 720, "Deep work"),
        (750, 810, "Lunch"),
    ] {
        amber_gpui::blocks::create_block(&db, DATE, s, e, title, None).unwrap();
    }
    // ERD — 데모 연결 하나와 상점 스키마 스냅샷(실제 업무 스키마는 쓰지 않는다)
    amber_gpui::dbconn::create_connection(
        &db,
        &amber_gpui::dbconn::ConnectionInput {
            name: "Demo DB".into(),
            kind: "mysql".into(),
            env: "dev".into(),
            host: "127.0.0.1".into(),
            port: 3306.,
            username: "demo".into(),
            tls: "preferred".into(),
            credential_command: String::new(),
            folder_path: "Demo DB".into(),
            schemas: serde_json::json!([{"name": "shop", "label": "Shop", "enabled": true, "starred": true}]),
        },
    )
    .unwrap();
    let col = |name: &str, ty: &str, ct: &str, key: &str, nullable: bool, comment: &str| {
        serde_json::json!({"name": name, "data_type": ty, "column_type": ct, "nullable": nullable, "key": key,
            "default_value": null, "extra": "", "comment": comment})
    };
    let table = |name: &str,
                 comment: &str,
                 columns: Vec<serde_json::Value>,
                 fks: serde_json::Value| {
        serde_json::json!({"name": name, "comment": comment, "rows_estimate": null, "columns": columns,
            "indexes": [], "foreign_keys": fks, "checks": []})
    };
    let fk = |name: &str, col: &str, to: &str| serde_json::json!({"name": name, "columns": [col], "ref_schema": "shop", "ref_table": to, "ref_columns": ["id"]});
    // 2분 전 — 오래되지 않아 열 때 다시 읽지 않는다
    let synced_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
        - 120_000;
    let snap = serde_json::json!({
        "amber": 1, "connection": "demo", "schema": "shop", "server": "MySQL 8.0.36", "synced_at": synced_at, "fingerprint": "demo",
        "tables": [
            table("users", "People who sign in", vec![col("id", "bigint", "bigint", "PRI", false, ""), col("email", "varchar", "varchar(255)", "UNI", false, "Sign-in email"), col("name", "varchar", "varchar(64)", "", true, "")], serde_json::json!([])),
            table("orders", "Customer orders", vec![col("id", "bigint", "bigint", "PRI", false, ""), col("user_id", "bigint", "bigint", "MUL", false, ""), col("status", "varchar", "enum('PENDING','PAID','CANCELED')", "", false, ""), col("created_at", "datetime", "datetime", "", false, "")], serde_json::json!([fk("fk_order_user", "user_id", "users")])),
            table("order_items", "Order lines", vec![col("id", "bigint", "bigint", "PRI", false, ""), col("order_id", "bigint", "bigint", "MUL", false, ""), col("product_id", "bigint", "bigint", "MUL", false, ""), col("quantity", "int", "int", "", false, "")], serde_json::json!([fk("fk_item_order", "order_id", "orders"), fk("fk_item_product", "product_id", "products")])),
            table("products", "Catalog", vec![col("id", "bigint", "bigint", "PRI", false, ""), col("name", "varchar", "varchar(120)", "", false, ""), col("price", "decimal", "decimal(12,2)", "", false, "")], serde_json::json!([])),
            table("payments", "", vec![col("id", "bigint", "bigint", "PRI", false, ""), col("order_id", "bigint", "bigint", "UNI", false, "One payment per order"), col("amount", "decimal", "decimal(12,2)", "", false, "")], serde_json::json!([fk("fk_payment_order", "order_id", "orders")])),
        ]
    });
    let erd = dir.join("vault/erd/Demo DB/shop");
    std::fs::create_dir_all(&erd).unwrap();
    std::fs::write(erd.join("schema.json"), snap.to_string()).unwrap();
    db_path.to_str().unwrap().to_string()
}

struct Shots {
    cx: VisualTestAppContext,
    window: WindowHandle<Spike>,
    out: PathBuf,
}

impl Shots {
    fn any(&self) -> AnyWindowHandle {
        self.window.into()
    }

    fn shot(&mut self, name: &str) {
        self.cx.run_until_parked();
        let img = self.cx.capture_screenshot(self.any()).unwrap();
        let path = self.out.join(format!("{name}.png"));
        img.save(&path).unwrap();
        eprintln!("{}", path.display());
    }

    fn row(&mut self, id: i64) -> Bounds<Pixels> {
        self.cx.run_until_parked();
        self.window
            .update(&mut self.cx, |s, _, _| {
                s.row_bounds.borrow().get(&id).copied()
            })
            .unwrap()
            .unwrap_or_else(|| panic!("줄 {id} 이 화면에 없다"))
    }

    /// 타임테이블 그리드에서 그 분의 창 좌표(가로는 칸 가운데)
    fn tt_at(&mut self, min: f64) -> Point<Pixels> {
        self.cx.run_until_parked();
        let g = self
            .window
            .update(&mut self.cx, |s, _, _| s.tt_grid.get())
            .unwrap();
        point(
            g.center().x,
            g.top() + px((min / 60. * crate::tt_card::HOUR_H as f64) as f32),
        )
    }

    /// 다시 그리고 뒤에서 도는 일(굽기 등)이 끝나길 잠깐씩 기다린다
    fn settle(&mut self) {
        for _ in 0..6 {
            let _ = self
                .window
                .update(&mut self.cx, |_, window, _| window.refresh());
            self.cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    }

    fn mouse(&mut self, p: Point<Pixels>, pressed: bool) {
        let w = self.any();
        let button = pressed.then_some(MouseButton::Left);
        self.cx.simulate_mouse_move(w, p, button, Modifiers::none());
        self.cx.run_until_parked();
    }
}

pub fn run(dir: &str) {
    let data = std::env::temp_dir().join(format!("amber-shots-{}", std::process::id()));
    let db = seed(&data);
    // 웹 하네스가 같은 데이터를 읽게 자리를 알린다
    eprintln!("data {}", data.display());
    let mut cx = VisualTestAppContext::with_asset_source(
        gpui_kit::platform::current_platform(false),
        Arc::new(assets::Assets),
    );
    let window = cx
        .open_offscreen_window(size(px(1440.), px(920.)), move |window, cx| {
            gpui_kit::init(cx);
            crate::note_edit::bind_keys(cx);
            amber_gpui::i18n::init(Some("en"));
            theme::apply(Some("dark"), WindowAppearance::Dark, cx);
            let db = Db::open(&db, Clock::System).unwrap();
            let view =
                cx.new(|cx| Spike::new(db, DATE.into(), None, None, Instant::now(), window, cx));
            // 찍기는 DB 와 키체인에 닿지 않는다 — ERD 동기화는 거절당한다
            view.update(cx, |s, _| {
                s.erd.db = Some(std::rc::Rc::new(crate::erd_sync::Refuse))
            });
            view
        })
        .unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let mut s = Shots {
        cx,
        window,
        out: PathBuf::from(dir),
    };
    s.shot("drag-0-rest");

    // hover — 왼쪽 여백에 손잡이가 뜬다
    let r5 = s.row(5);
    s.mouse(r5.center(), false);
    s.shot("drag-1-hover");

    // 낱개 줄을 묶음 안(둘째 하위 앞, 한 단 오른쪽)으로
    let grip = point(r5.left() - px(9.), r5.center().y);
    s.mouse(grip, false);
    let w = s.any();
    s.cx.simulate_mouse_down(w, grip, MouseButton::Left, Modifiers::none());
    s.mouse(point(grip.x, grip.y - px(4.)), true);
    let r3 = s.row(3);
    s.mouse(point(grip.x + px(26.), r3.top() + px(4.)), true);
    s.mouse(point(grip.x + px(26.), r3.top() + px(2.)), true);
    s.shot("drag-2-into-group");

    // 맨 위로
    let r1 = s.row(1);
    s.mouse(point(grip.x, r1.top() + px(2.)), true);
    s.mouse(point(grip.x, r1.top() + px(1.)), true);
    s.shot("drag-3-top");

    let at = point(grip.x, r1.top() + px(1.));
    s.cx.simulate_mouse_up(w, at, MouseButton::Left, Modifiers::none());
    s.shot("drag-4-dropped");

    timetable(&mut s);
    vacation(&mut s);
    let w = s.any();
    // 달력 판 폭 — 경계를 오른쪽으로 끄는 중(끄는 동안만 선이 보인다)
    let w_now = s.window.update(&mut s.cx, |sp, _, _| sp.cal_w).unwrap();
    let edge = point(px(68. + w_now), px(600.));
    s.mouse(edge, false);
    s.cx.simulate_mouse_down(w, edge, MouseButton::Left, Modifiers::none());
    s.mouse(point(edge.x + px(4.), edge.y), true);
    s.mouse(point(edge.x + px(160.), edge.y), true);
    s.shot("pane-resizing");
    s.cx.simulate_mouse_up(
        w,
        point(edge.x + px(160.), edge.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    reports(&mut s);
    notes(&mut s);
    comments(&mut s);
    note_ai(&mut s);
    diagrams(&mut s);
    erd(&mut s);
    concepts(&mut s);
    settings(&mut s);
    for (name, level) in [
        ("cal-month", crate::calendar::Level::Month),
        ("cal-year", crate::calendar::Level::Year),
    ] {
        s.window
            .update(&mut s.cx, |sp, _, cx| {
                sp.vac_open = false;
                sp.cal.level = level;
                cx.notify();
            })
            .unwrap();
        s.shot(name);
    }
}

/// 노트 탭 — 폴더를 펼치고 노트 하나를 연다, 트리 행에 손을 올린 채
fn notes(s: &mut Shots) {
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Notes;
            sp.reload_notes();
            sp.notes.expanded.insert("Engineering".into());
            sp.open_note("Engineering/Parity harness.md", cx);
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(160.), px(150.)), false);
    s.shot("notes-open");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.open_name_modal(
                crate::vault_pane::Which::Notes,
                crate::note_names::NameKind::NewFile,
                None,
                window,
                cx,
            );
            cx.notify();
        })
        .unwrap();
    s.shot("notes-name-modal");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.names = None;
            amber_gpui::prefs::set(
                &sp.db,
                "amber.recent-roots.notes",
                "[\"/Users/demo/rust-notes\"]",
            );
            sp.notes.root_menu = true;
            cx.notify();
        })
        .unwrap();
    s.shot("root-menu");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.notes.root_menu = false;
            sp.start_tree_find(crate::vault_pane::Which::Notes, window, cx);
            if let Some(f) = sp.notes.find.as_mut() {
                f.query = "re".into();
                f.idx = 1;
            }
            cx.notify();
        })
        .unwrap();
    s.shot("tree-find");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.find = None;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.root_menu = false;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.names = None;
            cx.notify();
        })
        .unwrap();
    // 트리 끌기 — Inbox 를 Engineering 위로
    s.cx.run_until_parked();
    let (from, to) = s
        .window
        .update(&mut s.cx, |sp, _, _| {
            let rows = sp.notes.tree_rows.borrow();
            (rows["Inbox.md"].0.center(), rows["Engineering"].0.center())
        })
        .unwrap();
    let w = s.any();
    s.mouse(from, false);
    s.cx.simulate_mouse_down(w, from, MouseButton::Left, Modifiers::none());
    s.mouse(point(from.x, from.y - px(6.)), true);
    s.mouse(to, true);
    s.mouse(point(to.x + px(2.), to.y), true);
    s.shot("tree-drag");
    s.cx.simulate_mouse_up(
        w,
        point(px(-40.), to.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    s.cx.run_until_parked();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.names = None;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.open_note("Reading.md", cx);
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(1300.), px(880.)), false);
    s.shot("notes-reading");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.start_page_find(window, cx);
            if let Some(f) = sp.notes.page_find.as_mut() {
                f.input
                    .update(cx, |st, cx| st.set_value("line", window, cx));
                f.query = "line".into();
                f.idx = 1;
            }
            sp.paint_page_find(cx);
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("page-find");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.close_page_find(cx);
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.scroll.set_offset(point(px(0.), px(-2000.)));
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("notes-reading-end");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.open_note("Engineering/Parity harness.md", cx);
            sp.start_note_edit(window, cx);
            cx.notify();
        })
        .unwrap();
    s.shot("notes-edit");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.edit = None;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.selected = None;
            cx.notify();
        })
        .unwrap();
    s.shot("notes-empty");
}

/// 인라인 질문 — 형광펜과 '질문 N', 목록, 이어 물은 스레드, 드래그 뒤 [질문] 버튼, 질문 패널
fn comments(s: &mut Shots) {
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.open_note("Engineering/Parity harness.md", cx);
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(1300.), px(880.)), false);
    s.shot("cmt-marks");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.cmts.list_open = true;
            cx.notify();
        })
        .unwrap();
    s.shot("cmt-list");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.open_thread("c2".into(), true, cx);
            cx.notify();
        })
        .unwrap();
    s.shot("cmt-thread");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.cmts.revise = Some((0, None));
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("cmt-revise");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.close_pop(cx);
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    // [질문] 버튼 — 오프스크린 창에선 끌기 선택이 그려지지 않는다(선택 흐름은 ui_tests 가 실제 끌기로 본다).
    // 그래서 놓은 자리와 앵커만 둔다
    let b = s
        .window
        .update(&mut s.cx, |sp, _, _| {
            sp.notes.marks.borrow().get(&0).copied()
        })
        .unwrap()
        .expect("첫 덩어리");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.cmts.sel = Some(crate::note_comments::Sel {
                anchor: "replays it against the Rust one".into(),
                occurrence: 0,
                at: point(b.left() + px(560.), b.top() + px(68.)),
                scroll_y: sp.scroll.offset().y,
            });
            cx.notify();
        })
        .unwrap();
    s.shot("cmt-fab");
    let fab = s
        .window
        .update(&mut s.cx, |sp, _, _| sp.notes.cmts.sel.clone())
        .unwrap();
    if let Some(sel) = fab {
        s.window
            .update(&mut s.cx, |sp, _, cx| {
                sp.open_pop(
                    crate::note_comments::Pop::Ask {
                        anchor: sel.anchor,
                        occurrence: sel.occurrence,
                    },
                    cx,
                );
                cx.notify();
            })
            .unwrap();
        s.shot("cmt-ask");
    }
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.close_pop(cx);
            cx.notify();
        })
        .unwrap();
}

/// 노트 AI — 지시 화면(칩, 참고 폴더), 쓰는 중(실시간 글), 검토(변경점), 배너와 트리 점
fn note_ai(s: &mut Shots) {
    use crate::note_ai::{Live, Outcome, StartCompose};
    let path = "Engineering/Parity harness.md";
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.open_note(path, cx);
            sp.open_ai_modal(cx);
            if let Some(m) = sp.notes.ai_modal.as_mut() {
                m.seed = Some("Add a section on how the Rust engine plugs in.".into());
                m.chosen = vec!["s:p1".into(), "p:1".into()];
                m.ref_on = vec!["/Users/me/src/amber".into()];
            }
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(1300.), px(880.)), false);
    s.shot("ai-prompt");
    let body = s
        .window
        .update(&mut s.cx, |sp, _, _| sp.notes.body.clone())
        .unwrap();
    let start = StartCompose {
        path: path.into(),
        title: "Parity harness".into(),
        markdown: body.clone(),
        typed: "Add a section on how the Rust engine plugs in.".into(),
        chosen: vec![],
        ref_dirs: vec!["/Users/me/src/amber".into()],
        instruction: "x".into(),
    };
    let now = chrono::Local::now().timestamp_millis();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let seq = sp.notes.ai.begin_compose(&start, "k", now - 8_000).unwrap();
            let live = Live {
                stream: "## 1. Why\n\nThe harness records every step of the TypeScript engine"
                    .into(),
                activity: Some((
                    (
                        "Read".into(),
                        Some("/Users/me/src/amber/parity/engine.ts".into()),
                    ),
                    now,
                )),
                changed: true,
                ..Default::default()
            };
            sp.notes.ai.pump(path, seq, &live);
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("ai-loading");
    let result = body.replacen(
        "## 2. Steps",
        "## 2. Rust engine\n\nSet `AMBER_PARITY_ENGINE` to the built binary.\n\n## 3. Steps",
        1,
    );
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let seq = sp.notes.ai.begin_continue(path, "k2", now).map(|x| x.0);
            let _ = seq;
            // 진행 중을 끝내고 결과를 놓는다
            sp.notes.ai.stop(path);
            let seq = sp
                .notes
                .ai
                .begin_compose(&start, "k3", now - 60_000)
                .unwrap();
            sp.notes.ai.finish_ok(
                path,
                seq,
                Outcome::Compose {
                    markdown: result.clone(),
                    truncated: false,
                    continued: false,
                    model: "sonnet".into(),
                },
                now - 30_000,
            );
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("ai-diff");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.ai_modal = None;
            cx.notify();
        })
        .unwrap();
    s.shot("ai-banner");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.dismiss_note_ai(path);
            sp.notes.ai.begin_compose(&start, "k4", now - 70_000);
            cx.notify();
        })
        .unwrap();
    s.shot("ai-banner-running");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.dismiss_note_ai(path);
            sp.open_span_modal(crate::note_span_ai::SpanMode::Section, None, cx);
            if let Some(m) = sp.notes.span_modal.as_mut() {
                m.picked = vec![1];
            }
            cx.notify();
        })
        .unwrap();
    s.shot("span-pick");
    let sections = amber_gpui::note_ai_text::split_sections(&body);
    let sec = sections[1].clone();
    let span = crate::note_ai::Span {
        section: true,
        start: sec.start,
        end: sec.end,
        label: sec.title.clone(),
    };
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            if let Some(m) = sp.notes.span_modal.as_mut() {
                m.picking = false;
                m.runs = vec![span.clone()];
                m.seed = Some("Explain what each command checks.".into());
            }
            cx.notify();
        })
        .unwrap();
    s.shot("span-prompt");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let p = crate::note_ai::StartSpan {
                path: path.into(),
                title: "Parity harness".into(),
                markdown: body.clone(),
                spans: vec![span.clone()],
                instruction: "x".into(),
                ref_dirs: vec![],
            };
            let seq = sp.notes.ai.begin_span(&p, "s1", now - 20_000).unwrap();
            sp.notes.ai.finish_ok(
                path,
                seq,
                Outcome::Span {
                    texts: vec!["## 2. Steps\n\n- Record with `pnpm parity:record` (writes every step)\n- Replay with `pnpm parity` (compares the Rust engine)".into()],
                    truncated: false,
                },
                now - 5_000,
            );
            cx.notify();
        })
        .unwrap();
    s.cx.run_until_parked();
    s.shot("span-review");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.span_modal = None;
            sp.dismiss_note_ai(path);
            cx.notify();
        })
        .unwrap();
}

/// 다이어그램 탭 — 빈 안내, 폴더를 펼치고 ERD 하나를 연 화면
fn diagrams(s: &mut Shots) {
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Diagrams;
            sp.reload_tree(crate::vault_pane::Which::Diagrams);
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(1300.), px(880.)), false);
    s.shot("dgm-empty");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.toggle_dir(crate::vault_pane::Which::Diagrams, "ERD");
            sp.open_diagram("ERD/Orders.mmd", cx);
            cx.notify();
        })
        .unwrap();
    // 판을 잰 다음 그림에서 맞추고, 굽기는 뒤에서 돈다 — 화면 밖 창은 스스로 다시 그리지 않는다
    s.settle();
    s.shot("dgm-open");
    // USER 를 골라 이웃(ORDER)만 남기고, 컬럼 패널을 연다
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let c = &mut sp.diagrams.canvas;
            let i = c
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().position(|n| n.text == "USER"))
                .expect("USER");
            c.select(i);
            c.show_cols = true;
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-selected");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.diagrams.canvas.deselect();
            cx.notify();
        })
        .unwrap();
    s.settle();
    // 찾기 — 'id' 가 든 라벨 둘(USER.id, ORDER.id, ORDER.user_id), 지금 것은 밑줄 + 굵은 상자
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.open_diagram_search(false, window, cx);
            if let Some(f) = sp.diagrams.canvas.search.as_ref() {
                f.input.update(cx, |st, cx| st.set_value("id", window, cx));
            }
            sp.diagrams.canvas.run_search("id");
            sp.diagrams.canvas.step_hit(1);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-search");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.diagrams.canvas.close_search();
            sp.diagrams.canvas.fit_now();
            cx.notify();
        })
        .unwrap();
    s.settle();
    // 커서 자리를 붙잡고 2.7배(휠 세 번) — 보이는 곳만 선명하게 다시 굽는다
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let c = &mut sp.diagrams.canvas;
            let v = c.viewport.get();
            c.zoom_by(
                2.744,
                (
                    f32::from(v.size.width) * 0.5,
                    f32::from(v.size.height) * 0.3,
                ),
            );
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-zoomed");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.start_diagram_edit(window, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-edit");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.cancel_diagram_edit();
            cx.notify();
        })
        .unwrap();
    // DDL → ERD — 지시 화면, 결과(다이어그램 탭), 결과 그림의 확대 뷰어
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.open_diagram_ai(window, cx);
            if let Some(a) = sp.diagrams.ai.as_mut() {
                a.ddl.update(cx, |st, cx| {
                    st.set_value(
                        "CREATE TABLE order_item (\n  id BIGINT NOT NULL AUTO_INCREMENT,\n  order_id BIGINT NOT NULL COMMENT 'ORDER.id',\n  PRIMARY KEY (id)\n);",
                        window,
                        cx,
                    )
                });
                a.chosen = vec![4];
            }
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-ai");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let result = "erDiagram\n  USER ||--o{ ORDER : places\n  ORDER ||--|{ ORDER_ITEM : contains\n  ORDER_ITEM {\n    bigint id PK\n    bigint order_id FK\n  }".to_string();
            if let Some(a) = sp.diagrams.ai.as_mut() {
                a.result = result.clone();
                a.step = crate::diagram_ai::Step::Preview;
            }
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("dgm-ai-preview");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            let fig = sp
                .diagrams
                .ai
                .as_ref()
                .and_then(|a| crate::figure::mermaid(&a.result).ok())
                .expect("결과 그림");
            sp.open_mmd_zoom(&fig, window, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("mmd-zoom");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.mmd_zoom = None;
            sp.diagrams.ai = None;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Notes;
            cx.notify();
        })
        .unwrap();
}

/// ERD 탭 — 데모 스키마를 고르고(오로라 정지 그림)
fn erd(s: &mut Shots) {
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Erd;
            sp.load_erd();
            let ulid = sp.erd.connections.as_ref().unwrap()[0].ulid.clone();
            sp.open_erd_schema(&ulid, "shop", cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.shot("erd-open");
    // 다른 무대 둘 — 비단, 물빛(라이브 유리)
    for stage in ["silk", "caustic"] {
        s.window
            .update(&mut s.cx, |sp, _, cx| {
                sp.set_erd_stage(stage);
                cx.notify();
            })
            .unwrap();
        s.settle();
        s.settle();
        s.shot(&format!("erd-glass-{stage}"));
    }
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.set_erd_stage("aurora");
            cx.notify();
        })
        .unwrap();
    s.settle();
    // 표 이름 = 오른쪽 패널(초점), 컬럼 = 정보창
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            use amber_gpui::resin::details::Act;
            sp.erd_act(&Act::Open("orders".into()), window, cx);
            sp.erd_act(&Act::Pick("orders".into(), "user_id".into()), window, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.settle();
    s.shot("erd-panel");
    // [보기] 판 — 키만, 타입 빼기
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.erd_escape(cx);
            sp.erd_escape(cx);
            sp.erd.canvas.view_open = true;
            sp.erd.view.columns = amber_gpui::resin::svg::Columns::Keys;
            sp.erd.view.hide = vec![amber_gpui::resin::svg::CardPart::Types];
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.settle();
    s.shot("erd-view");
    // ⌘F — "order" 는 표 둘(orders, order_items)과 컬럼 둘(order_items.order_id, payments.order_id)
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.erd.canvas.view_open = false;
            sp.erd.view.columns = amber_gpui::resin::svg::Columns::All;
            sp.erd.view.hide = Vec::new();
            sp.erd_open_find(window, cx);
            let f = sp.erd.canvas.find.as_mut().unwrap();
            f.query = "order".into();
            f.input.update(cx, |i, cx| i.set_value("order", window, cx));
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.erd_run_find(true, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.settle();
    s.shot("erd-find");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.erd_close_find(cx);
            cx.notify();
        })
        .unwrap();
    // DB 연결 모달 — 편집(키체인에 저장됨), 연결 확인 실패(찍기는 DB 가 없다), 스키마 고르기
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            let c = sp.erd.connections.as_ref().unwrap()[0].clone();
            sp.open_conn_modal(Some(c), window, cx);
            sp.conn_run_test(cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.shot("erd-conn-form");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            if let Some(m) = sp.erd.conn.as_mut() {
                m.test_error = None;
                m.test = Some(amber_core::dbconn::DbTestResult {
                    server: "MySQL 8.0.36".into(),
                    latency_ms: 14,
                    schemas: [("analytics", 0), ("shop", 5), ("shop_archive", 12)]
                        .iter()
                        .map(|(n, t)| amber_core::dbconn::SchemaInfo {
                            name: n.to_string(),
                            tables: *t,
                            error: None,
                        })
                        .collect(),
                });
                m.prefs = vec![
                    amber_gpui::dbconn::DbSchemaPref {
                        name: "analytics".into(),
                        label: String::new(),
                        enabled: false,
                        audit: None,
                        starred: None,
                    },
                    sp.erd.connections.as_ref().unwrap()[0].schemas[0].clone(),
                    amber_gpui::dbconn::DbSchemaPref {
                        name: "shop_archive".into(),
                        label: String::new(),
                        enabled: false,
                        audit: None,
                        starred: None,
                    },
                ];
            }
            sp.conn_enter_schemas(window, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.settle();
    s.shot("erd-conn-schemas");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.close_conn_modal(window, cx);
            cx.notify();
        })
        .unwrap();
    // [개요] — 통계, 테이블 목록, 참조 통계
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.erd_show_overview(window, cx);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("erd-overview");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.erd.overview = false;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Notes;
            cx.notify();
        })
        .unwrap();
}

/// 휴가 고르기 판 — 트리거 아래 유리 판, 첫 항목에 손을 올린 채
fn vacation(s: &mut Shots) {
    let w = s.any();
    s.cx.run_until_parked();
    let a = s
        .window
        .update(&mut s.cx, |sp, _, _| sp.vac_anchor.get())
        .unwrap();
    s.cx.simulate_click(w, a.center(), Modifiers::none());
    s.mouse(
        point(a.left() + px(30.), a.bottom() + px(6. + 4. + 16.)),
        false,
    );
    s.shot("vac-menu");
}

/// 타임테이블 — 08:30 이 판 위에 오게 굴려 두고 블록을 hover, 만들기, 겹쳐 옮기기, 제목 입력
fn timetable(s: &mut Shots) {
    let w = s.any();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.tt_scroll
                .set_offset(point(px(0.), px(-(8.5 * crate::tt_card::HOUR_H))));
            cx.notify();
        })
        .unwrap();
    let deep = s.tt_at(660.);
    s.mouse(deep, false);
    s.shot("tt-1-hover");

    // 빈 곳 13:00 → 14:00 끌기(만드는 중)
    let a = s.tt_at(905.);
    s.mouse(a, false);
    s.cx.simulate_mouse_down(w, a, MouseButton::Left, Modifiers::none());
    s.mouse(point(a.x, a.y + px(4.)), true);
    let b = s.tt_at(955.);
    s.mouse(b, true);
    s.shot("tt-2-creating");
    s.cx.simulate_mouse_up(w, b, MouseButton::Left, Modifiers::none());
    s.shot("tt-3-title");
    s.cx.simulate_keystrokes(w, "escape");

    // 점심을 깊은 작업 위로 — 놓기 전에 둘이 나란히 밀린다
    let lunch = s.tt_at(770.);
    s.mouse(lunch, false);
    s.cx.simulate_mouse_down(w, lunch, MouseButton::Left, Modifiers::none());
    s.mouse(point(lunch.x, lunch.y - px(4.)), true);
    let up = s.tt_at(680.);
    s.mouse(up, true);
    s.shot("tt-4-moving");
    s.cx.simulate_mouse_up(w, up, MouseButton::Left, Modifiers::none());
    s.shot("tt-5-moved");
}

/// 할 일 탭의 리포트 카드 — 완성(마크다운), 굴려서 할 일 카드를 밀어 올린 묶음, 수집 칩, 흐르는 글, 편집, 주간, 확인 창
fn reports(s: &mut Shots) {
    use crate::report_run::{Chip, ChipStatus, End, Live, Phase};
    use amber_core::report::CollectProgress;
    use amber_gpui::report;
    let body = "## 오늘 한 일\n\n- **동등성 하네스**: 리포트 본문 파일을 파일 상태까지 견주게 했다\n  - 일간, 주간 경로와 휴지통\n- 유리 배경을 Metal 로 옮기고 WebGL 기준 프레임과 비교했다\n\n## 내일 할 일\n\n- 리포트 패널을 GPUI 로\n- 설정 모달 뼈대\n\n## 메모\n\n- 카드 묶음: 리포트를 굴리면 할 일 카드가 먼저 밀려 올라간다\n- 다 올라간 뒤에 리포트 안이 구른다\n- 위로 굴리면 리포트가 맨 위에 닿은 뒤 할 일 카드가 내려온다\n- 다 보이는 리포트에서는 할 일을 치우지 않는다\n- 실패한 다시 생성은 직전 리포트를 지킨다\n- 주간은 평문 그대로\n";
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.section = crate::Section::Todo;
            sp.set_unit(crate::calendar::Unit::Day, window, cx);
            let rel = report::write_report_file(&sp.db, DATE, body).unwrap();
            report::upsert_report(
                &sp.db,
                &report::ReportRow {
                    date: DATE.into(),
                    file_path: rel,
                    sources_json: r#"[{"id":"github","rank":1,"ok":true,"items":7,"error":null},{"id":"ai_sessions","rank":2,"ok":false,"items":0,"error":"x"},{"id":"slack","rank":3,"ok":true,"items":0,"mcp":true,"error":null}]"#.into(),
                    provider: Some("claude".into()),
                    model: Some("claude-opus-5-5".into()),
                    duration_ms: Some(42_000.),
                },
            )
            .unwrap();
            sp.reports.panel.loaded_for = None;
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-0-done");

    // 리포트 위에서 아래로 굴린다 — 할 일 카드가 먼저 밀려 올라간다
    let at = s
        .window
        .update(&mut s.cx, |sp, _, _| {
            let b = sp.reports.panel.stack.bounds.get();
            point(b.center().x, b.bottom() - px(60.))
        })
        .unwrap();
    let w = s.any();
    for _ in 0..3 {
        s.cx.simulate_event(
            w,
            ScrollEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.), px(-90.))),
                ..Default::default()
            },
        );
        s.cx.run_until_parked();
    }
    s.settle();
    s.shot("report-1-collapsed");

    // 수집 중(칩) → 흐르는 글 — 스토어에 바로 넣는다(CLI 없이)
    let prev = amber_gpui::dates::shift_day(DATE, -1);
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.go_date(prev.clone());
            let chip = |id: &str, status| Chip {
                id: id.into(),
                status,
                items: 0,
                error: None,
            };
            let seq = sp
                .reports
                .runs
                .begin(
                    &prev,
                    Phase::Collecting,
                    vec![
                        Chip {
                            items: 4,
                            ..chip("todos", ChipStatus::Ok)
                        },
                        chip("github", ChipStatus::Pending),
                        chip("ai_sessions", ChipStatus::Pending),
                        chip("plugin:Slack:slack", ChipStatus::Mcp),
                    ],
                )
                .unwrap();
            sp.reports.runs.pump(
                &prev,
                seq,
                &Live {
                    progress: vec![CollectProgress {
                        id: "github".into(),
                        ok: true,
                        items: 12,
                        error: None,
                    }],
                    changed: true,
                    ..Live::default()
                },
                "k",
            );
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-2-collecting");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            let seq = 1;
            sp.reports.runs.pump(
                &prev,
                seq,
                &Live {
                    progress: vec![CollectProgress {
                        id: "ai_sessions".into(),
                        ok: false,
                        items: 0,
                        error: Some("세션 폴더 없음".into()),
                    }],
                    stream: "## 오늘 한 일\n\n- 리포트 패널을 옮기는 중\n- 칩과 흐르는 글".into(),
                    streaming: true,
                    changed: true,
                },
                "k",
            );
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-3-streaming");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.reports.runs.finish(&prev, 1, End::Empty);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-4-empty");

    // 편집 — 원문 | 미리보기
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.go_date(DATE.into());
            sp.reports.panel.loaded_for = None;
            sp.prepare_report(cx);
            sp.report_start_edit(body.into(), window, cx);
        })
        .unwrap();
    s.settle();
    s.shot("report-5-edit");

    // 다시 생성 확인 창, 실패(직전 리포트는 그대로)
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.reports.panel.edit = None;
            sp.reports.panel.ask = Some(crate::report_view::Ask::Regen);
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-6-regen-ask");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.reports.panel.ask = None;
            let seq = sp
                .reports
                .runs
                .begin(DATE, Phase::Streaming, Vec::new())
                .unwrap();
            sp.reports
                .runs
                .finish(DATE, seq, End::Error("사용량 한도에 닿았어요".into()));
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-7-failed-kept");

    // 주간 — 재료 칩(이틀 있음), 완성 평문
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.reports.runs.clear(DATE);
            sp.set_unit(crate::calendar::Unit::Week, window, cx);
            let week = amber_gpui::dates::week_start_of(DATE);
            let rel = report::write_weekly_report_file(
                &sp.db,
                &week,
                "이번 주\n  ㄴ 동등성 하네스를 리포트 파일까지 넓혔다\n  ㄴ 유리 배경을 Metal 로\n다음 주\n  ㄴ 설정 모달\n",
            )
            .unwrap();
            report::upsert_weekly_report(
                &sp.db,
                &report::ReportRow {
                    date: week,
                    file_path: rel,
                    sources_json: format!(r#"["{}","{DATE}"]"#, amber_gpui::dates::shift_day(DATE, -1)),
                    provider: Some("claude".into()),
                    model: Some("claude-opus-5-5".into()),
                    duration_ms: None,
                },
            )
            .unwrap();
            sp.reports.panel.loaded_for = None;
            sp.reports.panel.sources_for = None;
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("report-8-weekly");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.set_unit(crate::calendar::Unit::Day, window, cx);
            sp.go_date(DATE.into());
            cx.notify();
        })
        .unwrap();
}

/// 설정 › 데일리 리포트 — 가짜 감지(gh 붙음, 계정 둘, 세션 폴더, MCP 서버 둘), GitHub 펼침, 그다음 gh 로그인이 풀린 안내
fn settings(s: &mut Shots) {
    use crate::report_probe::{Account, Fake, GhState, Server, Tools};
    let fake = |gh: GhState| Fake {
        tools: Tools {
            gh: Some((
                "/opt/homebrew/bin/gh".into(),
                "gh version 2.83.0 (2026-10-01)".into(),
            )),
            claude_sessions: true,
            codex_sessions: false,
        },
        accounts: vec![
            Account {
                login: "jhzlo".into(),
                active: true,
            },
            Account {
                login: "work-account".into(),
                active: false,
            },
        ],
        gh,
        servers: vec![
            Server {
                name: "plugin:Slack:slack".into(),
                connected: true,
                status: "connected".into(),
                transport: "http".into(),
            },
            Server {
                name: "plugin:Notion:notion".into(),
                connected: false,
                status: "needs_auth".into(),
                transport: "http".into(),
            },
        ],
        clis: vec![
            crate::report_probe::Cli {
                id: "claude".into(),
                name: "Claude Code".into(),
                path: "/opt/homebrew/bin/claude".into(),
                version: "2.1.280 (Claude Code)".into(),
            },
            crate::report_probe::Cli {
                id: "codex".into(),
                name: "OpenAI Codex CLI".into(),
                path: "/opt/homebrew/bin/codex".into(),
                version: "codex-cli 0.153.4".into(),
            },
        ],
        auth: Some(crate::report_probe::Auth {
            supported: true,
            logged_in: Some(false),
        }),
        ..Fake::default()
    };
    let ok = GhState {
        ok: true,
        path: Some("/opt/homebrew/bin/gh".into()),
        version: Some("gh version 2.83.0 (2026-10-01)".into()),
        login: Some("jhzlo".into()),
        ..GhState::default()
    };
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            // MCP 절은 claude 연결일 때만 — 찍기 데이터 폴더의 가짜 경로(실행하지 않는다)
            let _ =
                amber_gpui::ai_config::connect_provider(&sp.db, "claude", "/usr/bin/true", None);
            sp.settings.probe = std::rc::Rc::new(fake(ok.clone()));
            sp.open_settings(cx);
        })
        .unwrap();
    s.settle();
    s.shot("settings-0-ai");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.settings.tab = crate::settings_view::Tab::Report;
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            if let Some(r) = sp.settings.rep.as_mut() {
                r.expanded = Some("github".into());
                r.cfg.mcp_picked = vec!["plugin:Slack:slack".into()];
            }
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("settings-1-report");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            if let Some(r) = sp.settings.rep.as_mut() {
                r.expanded = Some("ai_sessions".into());
            }
            sp.settings.pane.set_offset(point(px(0.), px(-1e6)));
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("settings-1b-report-bottom");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.settings.probe = std::rc::Rc::new(fake(GhState {
                ok: false,
                code: Some("GH_AUTH".into()),
                ..ok.clone()
            }));
            sp.close_settings(cx);
            sp.open_settings(cx);
            sp.settings.tab = crate::settings_view::Tab::Report;
        })
        .unwrap();
    s.settle();
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            if let Some(r) = sp.settings.rep.as_mut() {
                r.expanded = Some("github".into());
            }
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("settings-2-gh-guide");
    s.window
        .update(&mut s.cx, |sp, _, cx| sp.close_settings(cx))
        .unwrap();
}

/// 개념 탭 — 학습 중 목록, 그다음 전체 + 태그 'db' 로 거르고 한 줄을 고른 채
fn concepts(s: &mut Shots) {
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.section = crate::Section::Til;
            sp.reload_concepts();
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("concepts-0-learning");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.concepts.status = "all";
            sp.concepts.active_tags = vec!["db".into()];
            sp.reload_concepts();
            sp.concepts.selected = sp.concepts.list.first().and_then(|r| r.get("id")?.as_i64());
            cx.notify();
        })
        .unwrap();
    s.settle();
    s.shot("concepts-1-all-db");
    s.window
        .update(&mut s.cx, |sp, window, cx| sp.open_add_concept(window, cx))
        .unwrap();
    s.settle();
    s.shot("concepts-2-add");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.concepts.add = None;
            cx.notify();
        })
        .unwrap();
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.open_search(window, cx);
            let hits = crate::search_view::run_search(&sp.db, "log");
            if let Some(qs) = sp.search.as_mut() {
                // 코드로 넣은 값은 Change 를 내지 않는다 — 결과를 바로 채운다
                qs.input
                    .update(cx, |st, cx| st.set_value("log", window, cx));
                qs.hits = hits;
            }
        })
        .unwrap();
    s.settle();
    s.settle();
    s.shot("search-0");
    s.window
        .update(&mut s.cx, |sp, window, cx| {
            sp.close_search(window, cx);
            sp.open_widget(window, cx);
        })
        .unwrap();
    s.settle();
    let wh = s.window.update(&mut s.cx, |sp, _, _| sp.widget).unwrap();
    if let Some(wh) = wh {
        s.cx.run_until_parked();
        let img = s.cx.capture_screenshot(wh.into()).unwrap();
        let path = s.out.join("widget-0.png");
        img.save(&path).unwrap();
        eprintln!("{}", path.display());
    }
    s.window
        .update(&mut s.cx, |sp, window, cx| sp.close_search(window, cx))
        .unwrap();
}
