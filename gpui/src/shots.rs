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

fn seed(dir: &Path) -> (String, String) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("amber.db");
    let note = dir.join("note.md");
    std::fs::write(&note, "# note\n").unwrap();
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
    (
        db_path.to_str().unwrap().to_string(),
        note.to_str().unwrap().to_string(),
    )
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

    fn mouse(&mut self, p: Point<Pixels>, pressed: bool) {
        let w = self.any();
        let button = pressed.then_some(MouseButton::Left);
        self.cx.simulate_mouse_move(w, p, button, Modifiers::none());
        self.cx.run_until_parked();
    }
}

pub fn run(dir: &str) {
    let (db, note) =
        seed(&std::env::temp_dir().join(format!("amber-shots-{}", std::process::id())));
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
            cx.new(|cx| Spike::new(db, DATE.into(), &note, 0, None, Instant::now(), window, cx))
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
}
