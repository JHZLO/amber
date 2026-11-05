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
    // 읽기 충실도 견본 — 지금 앱(웹)과 나란히 찍어 맞춘다
    std::fs::write(
        notes.join("Reading.md"),
        include_str!("../assets/reading-sample.md"),
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
    let db = Db::open(db_path.to_str().unwrap(), Clock::System).unwrap();
    amber_gpui::migrate::run(&db).unwrap();
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
        .update(&mut s.cx, |sp, _, cx| {
            sp.open_note("Reading.md", cx);
            cx.notify();
        })
        .unwrap();
    s.mouse(point(px(1300.), px(880.)), false);
    s.shot("notes-reading");
    s.window
        .update(&mut s.cx, |sp, _, cx| {
            sp.notes.selected = None;
            cx.notify();
        })
        .unwrap();
    s.shot("notes-empty");
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
