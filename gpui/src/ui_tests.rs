// 화면 조작 시험대 — 헤드리스 창에 실제 시험판 화면(Spike)을 그리고 클릭, 키 입력, 끌기를 보내
// DB 에 남은 결과를 본다. 사람 손이 하던 확인(체크, 편집, 지우기 확인, 끌어 옮기기)을 여기서 한다.

use std::path::PathBuf;
use std::time::Instant;

use amber_gpui::db::{Clock, Db, get_i64};
use amber_gpui::todos;
use gpui_kit::{
    AppContext, Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowAppearance, WindowBounds, WindowOptions, point, px, size,
};

use crate::{Spike, theme};

const DATE: &str = "2026-11-05";

/// 임시 폴더에 마이그레이션을 깐 DB 와 빈 노트를 만들고, 할 일을 (내용, 부모 번호) 로 깐다
fn seeded(name: &str, items: &[(&str, Option<usize>)]) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("amber-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("amber.db");
    let db = Db::open(path.to_str().unwrap(), Clock::System).unwrap();
    amber_gpui::migrate::run(&db).unwrap();
    let mut ids = Vec::new();
    for (content, parent) in items {
        let parent = parent.map(|i| ids[i]);
        ids.push(todos::create_todo(&db, content, DATE, parent, "day").unwrap());
    }
    let note = dir.join("note.md");
    std::fs::write(&note, "# note\n").unwrap();
    (path, note)
}

fn open(cx: &mut TestAppContext, db: &PathBuf, note: &PathBuf) -> VisualTestContext {
    let (db, note) = (
        db.to_str().unwrap().to_string(),
        note.to_str().unwrap().to_string(),
    );
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        amber_gpui::i18n::init(Some("en"));
        theme::apply(Some("dark"), WindowAppearance::Dark, cx);
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(1440.), px(920.)),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        let (window, _) = gpui_kit::open_window(options, cx, move |window, cx| {
            let db = Db::open(&db, Clock::System).unwrap();
            cx.new(|cx| {
                Spike::new(
                    db,
                    DATE.to_string(),
                    &note,
                    0,
                    None,
                    Instant::now(),
                    window,
                    cx,
                )
            })
        })
        .unwrap();
        window
    });
    let vcx = VisualTestContext::from_window(window, cx);
    vcx.run_until_parked();
    vcx
}

/// 그 이름(debug_selector)의 요소 가운데를 누른다
fn click(cx: &mut VisualTestContext, name: &'static str) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(name)
        .unwrap_or_else(|| panic!("화면에 {name} 이 없다"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn bounds(cx: &mut VisualTestContext, name: &'static str) -> Bounds<Pixels> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(name)
        .unwrap_or_else(|| panic!("화면에 {name} 이 없다"))
}

/// 그 이름의 요소를 눌러 to 까지 네 번에 나눠 끌고 놓는다(첫 움직임이 끌기를 시작한다)
fn drag(cx: &mut VisualTestContext, name: &'static str, to: Point<Pixels>) {
    let from = bounds(cx, name).center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for i in 1..=4 {
        let f = i as f32 / 4.;
        let p = point(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
        cx.simulate_mouse_move(p, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
    }
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// 같은 부모 아래 형제 순서(sort_order)
fn order(db: &PathBuf, parent: Option<i64>) -> Vec<i64> {
    let db = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    db.select(
        "SELECT id FROM todos WHERE parent_id IS ?1 ORDER BY sort_order, id",
        &[parent.map_or(
            rusqlite::types::Value::Null,
            rusqlite::types::Value::Integer,
        )],
    )
    .unwrap()
    .iter()
    .filter_map(|r| get_i64(r, "id"))
    .collect()
}

fn row(db: &PathBuf, id: i64) -> Option<amber_gpui::db::Row> {
    let db = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    db.select(
        "SELECT * FROM todos WHERE id = ?1",
        &[rusqlite::types::Value::Integer(id)],
    )
    .unwrap()
    .into_iter()
    .next()
}

fn text(r: &amber_gpui::db::Row, key: &str) -> String {
    r.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn done_of(db: &PathBuf, id: i64) -> Option<i64> {
    let db = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    let rows = db
        .select(
            "SELECT done FROM todos WHERE id = ?1",
            &[rusqlite::types::Value::Integer(id)],
        )
        .unwrap();
    rows.first().and_then(|r| get_i64(r, "done"))
}

#[gpui_kit::test]
fn checking_a_row_marks_it_done(cx: &mut TestAppContext) {
    let (db, note) = seeded("check", &[("first", None), ("second", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "chk-2-false");
    assert_eq!(done_of(&db, 2), Some(1));
    assert_eq!(done_of(&db, 1), Some(0));
}

#[gpui_kit::test]
fn clicking_the_text_edits_it_and_enter_saves(cx: &mut TestAppContext) {
    let (db, note) = seeded("edit", &[("old name", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "txt-1-false");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("new name");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert_eq!(text(&row(&db, 1).unwrap(), "content"), "new name");
}

#[gpui_kit::test]
fn deleting_asks_first_and_cancel_keeps_the_row(cx: &mut TestAppContext) {
    let (db, note) = seeded(
        "delete",
        &[("group", None), ("child", Some(0)), ("other", None)],
    );
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "del-1");
    click(&mut ui, "confirm-cancel");
    assert!(row(&db, 1).is_some());
    click(&mut ui, "del-1");
    click(&mut ui, "confirm-delete");
    assert!(row(&db, 1).is_none(), "묶음이 지워진다");
    assert!(row(&db, 2).is_none(), "하위도 함께 지워진다");
    assert!(row(&db, 3).is_some());
}

#[gpui_kit::test]
fn adding_a_sub_item_puts_it_under_its_parent(cx: &mut TestAppContext) {
    let (db, note) = seeded("child", &[("parent", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "add-1");
    ui.simulate_input("kid");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    let kid = row(&db, 2).expect("하위 항목");
    assert_eq!(text(&kid, "content"), "kid");
    assert_eq!(get_i64(&kid, "parent_id"), Some(1));
    assert_eq!(text(&kid, "due_date"), DATE);
}

#[gpui_kit::test]
fn parking_takes_the_date_off(cx: &mut TestAppContext) {
    let (db, note) = seeded("park", &[("someday", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "park-1");
    assert!(get_i64(&row(&db, 1).unwrap(), "parked_at").is_some());
}

#[gpui_kit::test]
fn bringing_overdue_items_moves_them_to_the_day(cx: &mut TestAppContext) {
    let (db, note) = seeded("overdue", &[]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::create_todo(&d, "late", "2026-11-01", None, "day").unwrap();
    }
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "bring-all");
    assert_eq!(text(&row(&db, 1).unwrap(), "due_date"), DATE);
}

#[gpui_kit::test]
fn the_calendar_and_week_unit_change_what_is_shown(cx: &mut TestAppContext) {
    let (db, note) = seeded("calendar", &[("today item", None)]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::create_todo(&d, "next day item", "2026-11-06", None, "day").unwrap();
    }
    let mut ui = open(cx, &db, &note);
    assert!(ui.debug_bounds("txt-1-false").is_some());
    click(&mut ui, "cal-2026-11-06");
    assert!(ui.debug_bounds("txt-2-false").is_some(), "고른 날의 목록");
    assert!(ui.debug_bounds("txt-1-false").is_none());
    click(&mut ui, "unit-week");
    let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    assert_eq!(
        amber_gpui::prefs::get(&d, "amber.todo.unit").as_deref(),
        Some("week")
    );
    assert!(
        ui.debug_bounds("txt-2-false").is_none(),
        "주 단위는 주 할 일만"
    );
}

#[gpui_kit::test]
fn pulling_from_the_drawer_brings_it_to_today(cx: &mut TestAppContext) {
    let (db, note) = seeded("drawer", &[("someday", None)]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::park_subtree(&d, 1).unwrap();
    }
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "drawer");
    click(&mut ui, "tab-parked");
    click(&mut ui, "parked-1");
    let r = row(&db, 1).unwrap();
    assert!(get_i64(&r, "parked_at").is_none());
    let today = amber_gpui::dates::today_str(chrono::Local::now().timestamp_millis());
    assert_eq!(
        text(&r, "due_date"),
        today,
        "서랍에서 꺼내면 보고 있는 날이 아니라 오늘로"
    );
}

#[gpui_kit::test]
fn dragging_a_row_up_puts_it_first(cx: &mut TestAppContext) {
    let (db, note) = seeded("drag-order", &[("a", None), ("b", None), ("c", None)]);
    let mut ui = open(cx, &db, &note);
    let grip = bounds(&mut ui, "grip-3").center();
    let first = bounds(&mut ui, "txt-1-false");
    // 첫 줄의 가운데보다 위 = 맨 앞 자리. 가로로는 움직이지 않는다(깊이 그대로)
    drag(&mut ui, "grip-3", point(grip.x, first.top() + px(2.)));
    assert_eq!(order(&db, None), vec![3, 1, 2]);
}

#[gpui_kit::test]
fn dragging_right_makes_it_a_sub_item(cx: &mut TestAppContext) {
    let (db, note) = seeded("drag-indent", &[("a", None), ("b", None)]);
    let mut ui = open(cx, &db, &note);
    let grip = bounds(&mut ui, "grip-2").center();
    // 한 단(24px) 남짓 오른쪽 = 바로 위 줄의 하위
    drag(&mut ui, "grip-2", point(grip.x + px(30.), grip.y));
    assert_eq!(get_i64(&row(&db, 2).unwrap(), "parent_id"), Some(1));
    assert_eq!(order(&db, None), vec![1]);
}

#[gpui_kit::test]
fn a_group_moves_with_its_children(cx: &mut TestAppContext) {
    let (db, note) = seeded(
        "drag-group",
        &[("g", None), ("k1", Some(0)), ("k2", Some(0)), ("x", None)],
    );
    let mut ui = open(cx, &db, &note);
    let grip = bounds(&mut ui, "grip-1").center();
    let last = bounds(&mut ui, "txt-4-false");
    drag(&mut ui, "grip-1", point(grip.x, last.bottom() + px(12.)));
    assert_eq!(order(&db, None), vec![4, 1], "묶음이 x 아래로");
    assert_eq!(order(&db, Some(1)), vec![2, 3], "하위는 그대로 따라온다");
}

#[gpui_kit::test]
fn dropping_in_place_changes_nothing(cx: &mut TestAppContext) {
    let (db, note) = seeded("drag-noop", &[("a", None), ("b", None)]);
    let mut ui = open(cx, &db, &note);
    let grip = bounds(&mut ui, "grip-1").center();
    // 집은 줄이 빠지면 아래 줄이 그 자리로 올라온다 — 원래 가운데보다 위에서 놓아야 제자리다(지금 앱과 같다)
    drag(&mut ui, "grip-1", point(grip.x, grip.y - px(4.)));
    assert_eq!(order(&db, None), vec![1, 2]);
    assert!(get_i64(&row(&db, 2).unwrap(), "parent_id").is_none());
}
