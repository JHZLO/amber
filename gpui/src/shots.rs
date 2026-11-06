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
            amber_gpui::i18n::init(Some("en"));
            theme::apply(Some("dark"), WindowAppearance::Dark, cx);
            let db = Db::open(&db, Clock::System).unwrap();
            cx.new(|cx| Spike::new(db, DATE.into(), None, None, Instant::now(), window, cx))
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
    notes(&mut s);
    comments(&mut s);
    note_ai(&mut s);
    diagrams(&mut s);
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
