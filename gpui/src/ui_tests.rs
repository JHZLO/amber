// 화면 조작 시험대 — 헤드리스 창에 실제 시험판 화면(Spike)을 그리고 클릭, 키 입력, 끌기를 보내
// DB 에 남은 결과를 본다. 사람 손이 하던 확인(체크, 편집, 지우기 확인, 끌어 옮기기)을 여기서 한다.

use std::path::PathBuf;
use std::time::Instant;

use amber_gpui::db::{Clock, Db, get_i64};
use amber_gpui::todos;
use gpui_kit::{
    AppContext, Bounds, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollEvent,
    TestAppContext, VisualTestContext, WindowAppearance, WindowBounds, WindowOptions, point, px,
    size,
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
    drag_points(cx, from, to, Modifiers::none());
}

fn drag_points(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>, m: Modifiers) {
    cx.simulate_mouse_move(from, None, m);
    cx.simulate_mouse_down(from, MouseButton::Left, m);
    for i in 1..=4 {
        let f = i as f32 / 4.;
        let p = point(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
        cx.simulate_mouse_move(p, MouseButton::Left, m);
        cx.run_until_parked();
    }
    cx.simulate_mouse_up(to, MouseButton::Left, m);
    cx.run_until_parked();
}

/// 타임테이블 그리드에서 그 분(자정부터)의 창 y
fn tt_y(cx: &mut VisualTestContext, min: f64) -> Pixels {
    let grid = bounds(cx, "tt-grid");
    grid.top() + px((min / 60. * crate::tt_card::HOUR_H as f64) as f32)
}

/// 처음 스크롤은 시계(지금 - 1시간)를 따른다 — 그 분이 보이는 판 위쪽에 오게 굴린다
fn scroll_tt_to(cx: &mut VisualTestContext, min: f64) {
    let view = bounds(cx, "tt-scroll");
    let dy = view.top() + px(40.) - tt_y(cx, min);
    cx.simulate_event(ScrollEvent {
        position: view.center(),
        delta: ScrollDelta::Pixels(point(px(0.), dy)),
        ..Default::default()
    });
    cx.run_until_parked();
}

/// (date, start_min, end_min, title) 를 id 순으로
fn blocks_of(db: &PathBuf) -> Vec<(String, i64, i64, String)> {
    let db = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    db.select("SELECT * FROM time_blocks ORDER BY id", &[])
        .unwrap()
        .iter()
        .map(|r| {
            (
                text(r, "date"),
                get_i64(r, "start_min").unwrap(),
                get_i64(r, "end_min").unwrap(),
                text(r, "title"),
            )
        })
        .collect()
}

/// 블록 하나(10:00 - 11:00)를 깔고 연다
fn with_block(cx: &mut TestAppContext, name: &str) -> (PathBuf, VisualTestContext) {
    let (db, note) = seeded(name, &[]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        amber_gpui::blocks::create_block(&d, DATE, 600, 660, "Meeting", None).unwrap();
    }
    let mut ui = open(cx, &db, &note);
    scroll_tt_to(&mut ui, 570.);
    (db, ui)
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

#[gpui_kit::test]
fn dragging_on_the_grid_makes_a_block_and_asks_for_its_title(cx: &mut TestAppContext) {
    let (db, note) = seeded("tt-create", &[]);
    let mut ui = open(cx, &db, &note);
    scroll_tt_to(&mut ui, 570.);
    let grid = bounds(&mut ui, "tt-grid");
    let x = grid.center().x;
    let (y0, y1) = (tt_y(&mut ui, 605.), tt_y(&mut ui, 655.));
    drag_points(&mut ui, point(x, y0), point(x, y1), Modifiers::none());
    ui.simulate_input("Focus");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    // 누른 칸의 시작(10:00)부터 놓은 칸의 끝(11:00)까지, 15분 단위
    assert_eq!(
        blocks_of(&db),
        vec![(DATE.into(), 600, 660, "Focus".into())]
    );
}

#[gpui_kit::test]
fn dragging_a_block_moves_it_by_fifteen_minute_steps(cx: &mut TestAppContext) {
    let (db, mut ui) = with_block(cx, "tt-move");
    let from = bounds(&mut ui, "tt-block-1").center();
    let to = point(from.x, from.y + px(44.));
    drag_points(&mut ui, from, to, Modifiers::none());
    assert_eq!(
        blocks_of(&db),
        vec![(DATE.into(), 660, 720, "Meeting".into())]
    );
}

#[gpui_kit::test]
fn dragging_the_bottom_edge_stretches_it(cx: &mut TestAppContext) {
    let (db, mut ui) = with_block(cx, "tt-resize");
    let from = bounds(&mut ui, "tt-resize-1").center();
    let to = point(from.x, tt_y(&mut ui, 750.));
    drag_points(&mut ui, from, to, Modifiers::none());
    assert_eq!(
        blocks_of(&db),
        vec![(DATE.into(), 600, 750, "Meeting".into())]
    );
}

#[gpui_kit::test]
fn clicking_a_block_renames_it_and_a_click_outside_saves(cx: &mut TestAppContext) {
    let (db, mut ui) = with_block(cx, "tt-rename");
    click(&mut ui, "tt-block-1");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("Standup");
    // Enter 가 아니라 바깥을 눌러도 저장된다(지금 앱의 blur 저장)
    click(&mut ui, "today");
    assert_eq!(blocks_of(&db)[0].3, "Standup");
}

#[gpui_kit::test]
fn deleting_a_block_asks_first(cx: &mut TestAppContext) {
    let (db, mut ui) = with_block(cx, "tt-delete");
    click(&mut ui, "tt-del-1");
    click(&mut ui, "confirm-cancel");
    assert_eq!(blocks_of(&db).len(), 1);
    click(&mut ui, "tt-del-1");
    click(&mut ui, "confirm-delete");
    assert!(blocks_of(&db).is_empty());
}

#[gpui_kit::test]
fn option_drag_on_a_block_makes_a_new_one_over_it(cx: &mut TestAppContext) {
    let (db, mut ui) = with_block(cx, "tt-alt");
    let from = point(
        bounds(&mut ui, "tt-block-1").center().x,
        tt_y(&mut ui, 620.),
    );
    let to = point(from.x, tt_y(&mut ui, 640.));
    drag_points(&mut ui, from, to, Modifiers::alt());
    ui.simulate_keystrokes("escape");
    let got = blocks_of(&db);
    assert_eq!(
        got[0],
        (DATE.into(), 600, 660, "Meeting".into()),
        "아래 블록은 그대로"
    );
    assert_eq!((got[1].1, got[1].2), (615, 645));
}

#[gpui_kit::test]
fn a_click_outside_saves_a_todo_edit(cx: &mut TestAppContext) {
    let (db, note) = seeded("edit-out", &[("old name", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "txt-1-false");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("new name");
    click(&mut ui, "cal-2026-11-05");
    assert_eq!(text(&row(&db, 1).unwrap(), "content"), "new name");
}

#[gpui_kit::test]
fn a_click_outside_keeps_a_half_typed_sub_item(cx: &mut TestAppContext) {
    let (db, note) = seeded("child-out", &[("parent", None)]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "add-1");
    ui.simulate_input("kid");
    click(&mut ui, "cal-2026-11-05");
    let kid = row(&db, 2).expect("바깥을 눌러도 적던 하위 항목은 저장된다");
    assert_eq!(get_i64(&kid, "parent_id"), Some(1));
}

fn vacation_of(db: &PathBuf) -> Option<String> {
    let db = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    amber_gpui::vacations::get_vacation(&db, DATE).unwrap()
}

#[gpui_kit::test]
fn the_vacation_picker_sets_and_clears_the_day(cx: &mut TestAppContext) {
    let (db, note) = seeded("vacation", &[]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "vac");
    click(&mut ui, "vac-menu-1");
    assert_eq!(vacation_of(&db).as_deref(), Some("half"));
    assert!(ui.debug_bounds("vac-menu").is_none(), "고르면 닫힌다");
    // 바깥을 누르면 아무것도 바꾸지 않고 닫힌다
    click(&mut ui, "vac");
    click(&mut ui, "today");
    assert!(ui.debug_bounds("vac-menu").is_none());
    assert_eq!(vacation_of(&db).as_deref(), Some("half"));
    // 잡혀 있을 때만 맨 끝에 '해제'
    click(&mut ui, "vac");
    click(&mut ui, "vac-menu-5");
    assert_eq!(vacation_of(&db), None);
}

#[gpui_kit::test]
fn the_calendar_title_drills_up_to_months_and_years(cx: &mut TestAppContext) {
    let (db, note) = seeded("cal-pick", &[]);
    let mut ui = open(cx, &db, &note);
    click(&mut ui, "cal-title");
    assert!(ui.debug_bounds("pick-m-3").is_some(), "월 판");
    assert!(
        ui.debug_bounds("unit-day").is_none(),
        "월 판엔 일/주 단위가 없다"
    );
    click(&mut ui, "cal-title");
    // 2020 – 2029 판, 앞뒤 한 칸(2019, 2030)
    assert!(ui.debug_bounds("pick-y-2019").is_some());
    assert!(ui.debug_bounds("pick-y-2030").is_some());
    click(&mut ui, "cal-next");
    click(&mut ui, "pick-y-2031");
    click(&mut ui, "pick-m-3");
    // 2031년 3월의 일 판 — 3/1 칸이 있다
    assert!(ui.debug_bounds("cal-2031-03-01").is_some());
    assert!(ui.debug_bounds("unit-day").is_some());
}
