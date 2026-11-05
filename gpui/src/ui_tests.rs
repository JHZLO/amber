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

/// 임시 폴더에 마이그레이션을 깐 DB 를 만들고, 할 일을 (내용, 부모 번호) 로 깐다
fn seeded(name: &str, items: &[(&str, Option<usize>)]) -> PathBuf {
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
    path
}

fn open(cx: &mut TestAppContext, db: &PathBuf) -> VisualTestContext {
    open_with(cx, db, Clock::System)
}

/// 고정 시계 — 지우기가 진짜 휴지통 대신 데이터 폴더의 .trash/<n> 으로 간다
fn open_fixed(cx: &mut TestAppContext, db: &PathBuf) -> VisualTestContext {
    let at = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(1_700_000_000_000));
    open_with(cx, db, Clock::Fixed(at))
}

thread_local! {
    /// 마지막으로 연 화면 — 화면에 안 보이는 상태(찾기 결과 수 등)를 시험이 읽는다
    static LAST: std::cell::RefCell<Option<gpui_kit::Entity<Spike>>> = const { std::cell::RefCell::new(None) };
}

fn spike() -> gpui_kit::Entity<Spike> {
    LAST.with(|l| l.borrow().clone()).expect("연 화면")
}

fn open_with(cx: &mut TestAppContext, db: &PathBuf, clock: Clock) -> VisualTestContext {
    let db = db.to_str().unwrap().to_string();
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
        let (window, view) = gpui_kit::open_window(options, cx, move |window, cx| {
            let db = Db::open(&db, clock).unwrap();
            cx.new(|cx| Spike::new(db, DATE.to_string(), None, None, Instant::now(), window, cx))
        })
        .unwrap();
        LAST.with(|l| *l.borrow_mut() = Some(view));
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
    let db = seeded(name, &[]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        amber_gpui::blocks::create_block(&d, DATE, 600, 660, "Meeting", None).unwrap();
    }
    let mut ui = open(cx, &db);
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
    let db = seeded("check", &[("first", None), ("second", None)]);
    let mut ui = open(cx, &db);
    click(&mut ui, "chk-2-false");
    assert_eq!(done_of(&db, 2), Some(1));
    assert_eq!(done_of(&db, 1), Some(0));
}

#[gpui_kit::test]
fn clicking_the_text_edits_it_and_enter_saves(cx: &mut TestAppContext) {
    let db = seeded("edit", &[("old name", None)]);
    let mut ui = open(cx, &db);
    click(&mut ui, "txt-1-false");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("new name");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert_eq!(text(&row(&db, 1).unwrap(), "content"), "new name");
}

#[gpui_kit::test]
fn deleting_asks_first_and_cancel_keeps_the_row(cx: &mut TestAppContext) {
    let db = seeded(
        "delete",
        &[("group", None), ("child", Some(0)), ("other", None)],
    );
    let mut ui = open(cx, &db);
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
    let db = seeded("child", &[("parent", None)]);
    let mut ui = open(cx, &db);
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
    let db = seeded("park", &[("someday", None)]);
    let mut ui = open(cx, &db);
    click(&mut ui, "park-1");
    assert!(get_i64(&row(&db, 1).unwrap(), "parked_at").is_some());
}

#[gpui_kit::test]
fn bringing_overdue_items_moves_them_to_the_day(cx: &mut TestAppContext) {
    let db = seeded("overdue", &[]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::create_todo(&d, "late", "2026-11-01", None, "day").unwrap();
    }
    let mut ui = open(cx, &db);
    click(&mut ui, "bring-all");
    assert_eq!(text(&row(&db, 1).unwrap(), "due_date"), DATE);
}

#[gpui_kit::test]
fn the_calendar_and_week_unit_change_what_is_shown(cx: &mut TestAppContext) {
    let db = seeded("calendar", &[("today item", None)]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::create_todo(&d, "next day item", "2026-11-06", None, "day").unwrap();
    }
    let mut ui = open(cx, &db);
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
    let db = seeded("drawer", &[("someday", None)]);
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        todos::park_subtree(&d, 1).unwrap();
    }
    let mut ui = open(cx, &db);
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
    let db = seeded("drag-order", &[("a", None), ("b", None), ("c", None)]);
    let mut ui = open(cx, &db);
    let grip = bounds(&mut ui, "grip-3").center();
    let first = bounds(&mut ui, "txt-1-false");
    // 첫 줄의 가운데보다 위 = 맨 앞 자리. 가로로는 움직이지 않는다(깊이 그대로)
    drag(&mut ui, "grip-3", point(grip.x, first.top() + px(2.)));
    assert_eq!(order(&db, None), vec![3, 1, 2]);
}

#[gpui_kit::test]
fn dragging_right_makes_it_a_sub_item(cx: &mut TestAppContext) {
    let db = seeded("drag-indent", &[("a", None), ("b", None)]);
    let mut ui = open(cx, &db);
    let grip = bounds(&mut ui, "grip-2").center();
    // 한 단(24px) 남짓 오른쪽 = 바로 위 줄의 하위
    drag(&mut ui, "grip-2", point(grip.x + px(30.), grip.y));
    assert_eq!(get_i64(&row(&db, 2).unwrap(), "parent_id"), Some(1));
    assert_eq!(order(&db, None), vec![1]);
}

#[gpui_kit::test]
fn a_group_moves_with_its_children(cx: &mut TestAppContext) {
    let db = seeded(
        "drag-group",
        &[("g", None), ("k1", Some(0)), ("k2", Some(0)), ("x", None)],
    );
    let mut ui = open(cx, &db);
    let grip = bounds(&mut ui, "grip-1").center();
    let last = bounds(&mut ui, "txt-4-false");
    drag(&mut ui, "grip-1", point(grip.x, last.bottom() + px(12.)));
    assert_eq!(order(&db, None), vec![4, 1], "묶음이 x 아래로");
    assert_eq!(order(&db, Some(1)), vec![2, 3], "하위는 그대로 따라온다");
}

#[gpui_kit::test]
fn dropping_in_place_changes_nothing(cx: &mut TestAppContext) {
    let db = seeded("drag-noop", &[("a", None), ("b", None)]);
    let mut ui = open(cx, &db);
    let grip = bounds(&mut ui, "grip-1").center();
    // 집은 줄이 빠지면 아래 줄이 그 자리로 올라온다 — 원래 가운데보다 위에서 놓아야 제자리다(지금 앱과 같다)
    drag(&mut ui, "grip-1", point(grip.x, grip.y - px(4.)));
    assert_eq!(order(&db, None), vec![1, 2]);
    assert!(get_i64(&row(&db, 2).unwrap(), "parent_id").is_none());
}

#[gpui_kit::test]
fn dragging_on_the_grid_makes_a_block_and_asks_for_its_title(cx: &mut TestAppContext) {
    let db = seeded("tt-create", &[]);
    let mut ui = open(cx, &db);
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
    let db = seeded("edit-out", &[("old name", None)]);
    let mut ui = open(cx, &db);
    click(&mut ui, "txt-1-false");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("new name");
    click(&mut ui, "cal-2026-11-05");
    assert_eq!(text(&row(&db, 1).unwrap(), "content"), "new name");
}

#[gpui_kit::test]
fn a_click_outside_keeps_a_half_typed_sub_item(cx: &mut TestAppContext) {
    let db = seeded("child-out", &[("parent", None)]);
    let mut ui = open(cx, &db);
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
    let db = seeded("vacation", &[]);
    let mut ui = open(cx, &db);
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
    let db = seeded("cal-pick", &[]);
    let mut ui = open(cx, &db);
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

#[gpui_kit::test]
fn dragging_the_pane_edge_resizes_the_calendar_and_remembers_it(cx: &mut TestAppContext) {
    let db = seeded("pane", &[]);
    let mut ui = open(cx, &db);
    let edge = bounds(&mut ui, "resizer-amber.todo.cal-width").center();
    drag(
        &mut ui,
        "resizer-amber.todo.cal-width",
        point(edge.x + px(100.), edge.y),
    );
    let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    // 기본 460 + 100. 잡은 지점을 존중하므로 손잡이 어디를 잡았든 움직인 만큼만
    assert_eq!(
        amber_gpui::prefs::get(&d, "amber.todo.cal-width").as_deref(),
        Some("560")
    );
    let edge = bounds(&mut ui, "resizer-amber.todo.cal-width").center();
    drag(
        &mut ui,
        "resizer-amber.todo.cal-width",
        point(edge.x - px(900.), edge.y),
    );
    assert_eq!(
        amber_gpui::prefs::get(&d, "amber.todo.cal-width").as_deref(),
        Some("260"),
        "최소 폭에서 멈춘다"
    );
}

#[gpui_kit::test]
fn releasing_outside_the_window_still_drops(cx: &mut TestAppContext) {
    let db = seeded("drag-outside", &[("a", None), ("b", None)]);
    let mut ui = open(cx, &db);
    let grip = bounds(&mut ui, "grip-2").center();
    let first = bounds(&mut ui, "txt-1-false");
    // 맨 앞 자리까지 끈 뒤 창 밖(왼쪽)에서 놓는다 — 슬롯은 마지막으로 고른 자리
    let from = grip;
    let mid = point(grip.x, first.top() + px(2.));
    ui.simulate_mouse_move(from, None, Modifiers::none());
    ui.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for p in [
        point(from.x, from.y - px(5.)),
        mid,
        point(mid.x, mid.y - px(1.)),
    ] {
        ui.simulate_mouse_move(p, MouseButton::Left, Modifiers::none());
        ui.run_until_parked();
    }
    ui.simulate_mouse_up(point(px(-50.), mid.y), MouseButton::Left, Modifiers::none());
    ui.run_until_parked();
    assert_eq!(order(&db, None), vec![2, 1]);
}

/// 보관함(데이터 폴더의 vault/notes)에 노트를 깐다
fn vault_note(db: &PathBuf, rel: &str, body: &str) {
    let path = db.parent().unwrap().join("vault/notes").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[gpui_kit::test]
fn the_tree_opens_folders_and_notes(cx: &mut TestAppContext) {
    let db = seeded("notes-open", &[]);
    vault_note(&db, "Eng/Parity.md", "# Parity\n\nbody\n");
    vault_note(&db, "Inbox.md", "# Inbox\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    assert!(ui.debug_bounds("tree-Inbox.md").is_some());
    assert!(
        ui.debug_bounds("tree-Eng/Parity.md").is_none(),
        "접힌 폴더의 노트는 안 보인다"
    );
    click(&mut ui, "tree-Eng");
    click(&mut ui, "tree-Eng/Parity.md");
    click(&mut ui, "note-copy");
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("# Parity\n\nbody\n"),
        "복사는 노트 전문"
    );
    // 폴더를 다시 누르면 접힌다
    click(&mut ui, "tree-Eng");
    assert!(ui.debug_bounds("tree-Eng/Parity.md").is_none());
}

#[gpui_kit::test]
fn deleting_a_note_asks_then_moves_it_to_the_trash(cx: &mut TestAppContext) {
    let db = seeded("notes-delete", &[]);
    vault_note(&db, "Inbox.md", "# Inbox\n");
    let mut ui = open_fixed(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-Inbox.md");
    click(&mut ui, "note-delete");
    click(&mut ui, "confirm-cancel");
    let dir = db.parent().unwrap();
    assert!(dir.join("vault/notes/Inbox.md").exists());
    click(&mut ui, "note-delete");
    click(&mut ui, "confirm-delete");
    assert!(!dir.join("vault/notes/Inbox.md").exists());
    assert!(
        dir.join(".trash/1/Inbox.md").exists(),
        "지우지 않고 휴지통으로"
    );
    assert!(
        ui.debug_bounds("note-copy").is_none(),
        "열려 있던 노트는 닫힌다"
    );
    assert!(ui.debug_bounds("tree-Inbox.md").is_none());
}

#[gpui_kit::test]
fn the_contents_list_jumps_to_a_heading(cx: &mut TestAppContext) {
    let db = seeded("notes-toc", &[]);
    let mut long = String::from("# Top\n\nintro\n\n");
    for i in 1..=4 {
        long.push_str(&format!("## {i}. Part\n\n"));
        long.push_str(&"line\n\n".repeat(30));
    }
    vault_note(&db, "Long.md", &long);
    vault_note(&db, "Short.md", "# Only one heading\n\nbody\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-Short.md");
    assert!(
        ui.debug_bounds("toc-0").is_none(),
        "제목이 하나면 목차가 없다"
    );
    click(&mut ui, "tree-Long.md");
    let before = bounds(&mut ui, "note-block-3").top();
    click(&mut ui, "toc-3");
    let view = bounds(&mut ui, "note-scroll").top();
    let after = bounds(&mut ui, "note-block-3").top();
    assert!(after < before, "본문이 그 절까지 굴러간다");
    assert!(
        (f32::from(after - view) - 18.).abs() < 1.,
        "그 절 제목이 판 위 18px 에 온다"
    );
}

#[gpui_kit::test]
fn a_details_toggle_opens_and_closes(cx: &mut TestAppContext) {
    let db = seeded("notes-toggle", &[]);
    vault_note(
        &db,
        "T.md",
        "# T\n\n<details>\n<summary>More</summary>\n\n<details open>\n<summary>Inner</summary>\n\ndeep\n\n</details>\n\n</details>\n",
    );
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-T.md");
    assert!(ui.debug_bounds("toggle-1").is_some(), "접힌 카드");
    assert!(
        ui.debug_bounds("toggle-1/0").is_none(),
        "접힌 카드 안은 안 그린다"
    );
    click(&mut ui, "toggle-1");
    assert!(
        ui.debug_bounds("toggle-1/0").is_some(),
        "펴면 안쪽 토글(open)도 보인다"
    );
    click(&mut ui, "toggle-1");
    assert!(ui.debug_bounds("toggle-1/0").is_none());
}

#[gpui_kit::test]
fn the_path_line_copies_the_note_path(cx: &mut TestAppContext) {
    let db = seeded("notes-crumb", &[]);
    vault_note(&db, "Eng/P.md", "# P\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-Eng");
    click(&mut ui, "tree-Eng/P.md");
    click(&mut ui, "note-crumb");
    // 데이터 폴더는 실제 경로로 풀린다(/var → /private/var)
    let want = std::fs::canonicalize(db.parent().unwrap().join("vault/notes/Eng/P.md")).unwrap();
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        want.to_str()
    );
    ui.write_to_clipboard(gpui_kit::ClipboardItem::new_string("x".into()));
    ui.simulate_keystrokes("cmd-alt-c");
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        want.to_str(),
        "⌥⌘C 도 같다"
    );
}

fn read_note(db: &PathBuf, rel: &str) -> String {
    std::fs::read_to_string(db.parent().unwrap().join("vault/notes").join(rel)).unwrap()
}

/// 노트 탭에서 그 노트를 열고 편집을 시작해 본문을 통째로 바꾼다
fn edit_note(ui: &mut VisualTestContext, rel: &'static str, text: &str) {
    click(ui, "app.rail.notes");
    click(ui, rel);
    click(ui, "note-edit");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input(text);
    ui.run_until_parked();
}

#[gpui_kit::test]
fn editing_and_saving_writes_the_file(cx: &mut TestAppContext) {
    let db = seeded("notes-edit", &[]);
    vault_note(&db, "A.md", "# A\n\nold\n");
    let mut ui = open(cx, &db);
    edit_note(&mut ui, "tree-A.md", "# A\n\nnew body\n");
    assert!(ui.debug_bounds("note-preview").is_some(), "원문 | 미리보기");
    click(&mut ui, "note-save");
    assert_eq!(read_note(&db, "A.md"), "# A\n\nnew body\n");
    assert!(
        ui.debug_bounds("note-save").is_none(),
        "저장하면 읽기로 돌아간다"
    );
    // ⌘S 도 같다
    click(&mut ui, "note-edit");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("# A\n\nby shortcut\n");
    ui.simulate_keystrokes("cmd-s");
    ui.run_until_parked();
    assert_eq!(read_note(&db, "A.md"), "# A\n\nby shortcut\n");
}

#[gpui_kit::test]
fn switching_notes_with_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let db = seeded("notes-unsaved", &[]);
    vault_note(&db, "A.md", "# A\n");
    vault_note(&db, "B.md", "# B\n");
    let mut ui = open(cx, &db);
    edit_note(&mut ui, "tree-A.md", "# A changed\n");
    click(&mut ui, "tree-B.md");
    click(&mut ui, "unsaved-keep");
    assert!(ui.debug_bounds("note-save").is_some(), "계속 편집");
    click(&mut ui, "tree-B.md");
    click(&mut ui, "unsaved-discard");
    assert!(ui.debug_bounds("note-save").is_none());
    assert_eq!(read_note(&db, "A.md"), "# A\n", "버린 초안은 쓰지 않는다");
    click(&mut ui, "note-copy");
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("# B\n"),
        "B 가 열렸다"
    );
}

#[gpui_kit::test]
fn saving_over_a_newer_disk_file_asks_first(cx: &mut TestAppContext) {
    let db = seeded("notes-conflict", &[]);
    vault_note(&db, "A.md", "# A\n");
    let mut ui = open(cx, &db);
    edit_note(&mut ui, "tree-A.md", "# mine\n");
    // 밖(다른 편집기)에서 먼저 고쳤다
    std::thread::sleep(std::time::Duration::from_millis(20));
    vault_note(&db, "A.md", "# theirs\n");
    click(&mut ui, "note-save");
    assert!(
        ui.debug_bounds("conflict-overwrite").is_some(),
        "조용히 덮지 않는다"
    );
    assert_eq!(read_note(&db, "A.md"), "# theirs\n");
    click(&mut ui, "conflict-overwrite");
    assert_eq!(read_note(&db, "A.md"), "# mine\n");
}

fn vault_has(db: &PathBuf, rel: &str) -> bool {
    db.parent().unwrap().join("vault/notes").join(rel).exists()
}

#[gpui_kit::test]
fn a_new_note_path_creates_folders_and_opens_for_editing(cx: &mut TestAppContext) {
    let db = seeded("notes-new", &[]);
    vault_note(&db, "Inbox.md", "# Inbox\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "notes-new-note");
    ui.simulate_input("CS/네트워크");
    click(&mut ui, "name-submit");
    assert_eq!(
        read_note(&db, "CS/네트워크.md"),
        "# 네트워크\n\n",
        "중간 폴더까지, 제목 틀"
    );
    assert!(
        ui.debug_bounds("tree-CS/네트워크.md").is_some(),
        "만든 자리까지 펼친다"
    );
    assert!(ui.debug_bounds("note-save").is_some(), "바로 편집");
}

#[gpui_kit::test]
fn bad_names_and_duplicates_stay_in_the_modal(cx: &mut TestAppContext) {
    let db = seeded("notes-badname", &[]);
    vault_note(&db, "A.md", "# A\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "notes-new-note");
    ui.simulate_input("a:b");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert!(
        ui.debug_bounds("name-submit").is_some(),
        "금지 글자면 닫히지 않는다"
    );
    assert!(!vault_has(&db, "a:b.md"));
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("A");
    click(&mut ui, "name-submit");
    assert!(
        ui.debug_bounds("name-submit").is_some(),
        "같은 이름이면 닫히지 않는다"
    );
    click(&mut ui, "name-cancel");
    assert!(ui.debug_bounds("name-submit").is_none());
}

#[gpui_kit::test]
fn a_new_folder_goes_where_the_location_says(cx: &mut TestAppContext) {
    let db = seeded("notes-newfolder", &[]);
    vault_note(&db, "CS/a.md", "# a\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "notes-new-folder");
    click(&mut ui, "name-dir");
    // 0 = 루트, 1 = /CS
    click(&mut ui, "name-dir-menu-1");
    ui.simulate_input("Sub");
    click(&mut ui, "name-submit");
    assert!(db.parent().unwrap().join("vault/notes/CS/Sub").is_dir());
    assert!(ui.debug_bounds("tree-CS/Sub").is_some(), "펼쳐서 보여 준다");
}

#[gpui_kit::test]
fn renaming_from_the_row_keeps_the_open_note(cx: &mut TestAppContext) {
    let db = seeded("notes-rename", &[]);
    vault_note(&db, "A.md", "# A\n\nbody\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-A.md");
    click(&mut ui, "row-rename-A.md");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("B");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert!(!vault_has(&db, "A.md") && vault_has(&db, "B.md"));
    assert!(ui.debug_bounds("tree-B.md").is_some());
    // 제목을 눌러도 같은 모달
    click(&mut ui, "note-title");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("C");
    click(&mut ui, "name-submit");
    assert!(vault_has(&db, "C.md"));
    click(&mut ui, "note-copy");
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("# A\n\nbody\n"),
        "열린 노트가 새 이름을 따라간다"
    );
}

#[gpui_kit::test]
fn dragging_a_note_onto_a_folder_moves_it(cx: &mut TestAppContext) {
    let db = seeded("tree-dnd", &[]);
    vault_note(&db, "A.md", "# A\n");
    vault_note(&db, "CS/x.md", "# x\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    let to = bounds(&mut ui, "tree-CS").center();
    drag(&mut ui, "tree-A.md", to);
    assert!(!vault_has(&db, "A.md") && vault_has(&db, "CS/A.md"));
    assert!(
        ui.debug_bounds("tree-CS/A.md").is_some(),
        "옮겨 간 자리를 펼친다"
    );
}

#[gpui_kit::test]
fn the_tree_margin_is_the_root_and_a_folder_cannot_go_inside_itself(cx: &mut TestAppContext) {
    let db = seeded("tree-dnd-root", &[]);
    vault_note(&db, "CS/x.md", "# x\n");
    vault_note(&db, "CS/Sub/y.md", "# y\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-CS");
    // 폴더를 자기 안의 파일 위로 = 자기 자신 — 옮기지 않는다
    let inside = bounds(&mut ui, "tree-CS/x.md").center();
    drag(&mut ui, "tree-CS", inside);
    assert!(vault_has(&db, "CS/x.md"));
    // 트리 아래 여백 = 루트
    let tree = bounds(&mut ui, "notes-tree");
    let x = tree.center().x;
    drag(&mut ui, "tree-CS/x.md", point(x, tree.bottom() - px(12.)));
    assert!(vault_has(&db, "x.md") && !vault_has(&db, "CS/x.md"));
}

#[gpui_kit::test]
fn the_root_picker_switches_workspace_folders(cx: &mut TestAppContext) {
    let db = seeded("notes-root", &[]);
    vault_note(&db, "Inbox.md", "# Inbox\n");
    // 전에 열었던 작업 폴더(최근 목록에 있다)
    let ws = db.parent().unwrap().join("work-folder");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("Ext.md"), "# Ext\n").unwrap();
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        let recents = serde_json::to_string(&[ws.to_str().unwrap()]).unwrap();
        amber_gpui::prefs::set(&d, "amber.recent-roots.notes", &recents);
    }
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-Inbox.md");
    click(&mut ui, "note-edit");
    ui.simulate_input("dirty ");
    ui.run_until_parked();
    // 기본 보관함이면 판에 [최근 폴더, 폴더 열기]
    click(&mut ui, "root-picker");
    click(&mut ui, "root-menu-0");
    assert!(
        ui.debug_bounds("unsaved-discard").is_some(),
        "고치던 초안이 있으면 먼저 묻는다"
    );
    click(&mut ui, "unsaved-discard");
    assert!(ui.debug_bounds("tree-Ext.md").is_some(), "고른 폴더의 트리");
    assert!(ui.debug_bounds("tree-Inbox.md").is_none());
    // 다른 폴더면 맨 위에 기본 보관함
    click(&mut ui, "root-picker");
    click(&mut ui, "root-menu-0");
    assert!(
        ui.debug_bounds("tree-Inbox.md").is_some(),
        "기본 보관함으로 돌아왔다"
    );
    let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    assert_eq!(
        amber_gpui::prefs::get(&d, "amber.root.notes").as_deref(),
        Some("")
    );
}

#[gpui_kit::test]
fn tree_find_reaches_into_collapsed_folders(cx: &mut TestAppContext) {
    let db = seeded("tree-find", &[]);
    vault_note(&db, "CS/Network/TCP 혼잡 제어.md", "# TCP\n\nbody\n");
    vault_note(&db, "CS/tcp-handshake.md", "# hs\n");
    vault_note(&db, "Inbox.md", "# Inbox\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    // 트리 판을 누르면 그 뒤의 ⌘F 가 트리 찾기로 온다
    click(&mut ui, "tree-Inbox.md");
    ui.simulate_keystrokes("cmd-f");
    ui.simulate_input("tcp");
    ui.run_until_parked();
    assert!(
        ui.debug_bounds("tree-CS/Network/TCP 혼잡 제어.md")
            .is_some(),
        "접힌 폴더 안까지 찾아 조상을 펼친다"
    );
    assert!(
        ui.debug_bounds("tree-Inbox.md").is_none(),
        "안 맞는 것은 숨긴다"
    );
    // 전위 순서: CS/Network/TCP… 다음 CS/tcp-handshake
    ui.simulate_keystrokes("down");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    click(&mut ui, "note-copy");
    assert_eq!(
        ui.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("# hs\n"),
        "두 번째 결과를 열었다"
    );
    assert!(
        ui.debug_bounds("tree-find-count").is_none(),
        "파일을 열면 찾기를 닫는다"
    );
    assert!(ui.debug_bounds("tree-Inbox.md").is_some());
    // 맞는 게 없으면 안내
    click(&mut ui, "notes-find");
    ui.simulate_input("zzz");
    ui.run_until_parked();
    assert!(ui.debug_bounds("tree-Inbox.md").is_none());
    ui.simulate_keystrokes("escape");
    ui.run_until_parked();
    assert!(
        ui.debug_bounds("tree-Inbox.md").is_some(),
        "Esc 로 닫으면 원래 트리"
    );
}

#[gpui_kit::test]
fn command_f_in_the_note_finds_rendered_text(cx: &mut TestAppContext) {
    let db = seeded("page-find", &[]);
    vault_note(
        &db,
        "T.md",
        "# Title\n\nfoo **bar** foo\n\n## 2. Next\n\nFOO end\n",
    );
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-T.md");
    // 트리를 누른 뒤 본문을 누르면 ⌘F 는 노트 안 찾기
    click(&mut ui, "note-scroll");
    ui.simulate_keystrokes("cmd-f");
    ui.simulate_input("foo");
    ui.run_until_parked();
    assert!(ui.debug_bounds("page-find-count").is_some());
    assert!(
        ui.debug_bounds("tree-find-count").is_none(),
        "트리 찾기가 아니다"
    );
    let hits = |ui: &mut VisualTestContext| {
        ui.update(|_, cx| {
            let n = &spike().read(cx).notes;
            let f = n.page_find.as_ref().unwrap();
            (f.hits.len(), f.idx)
        })
    };
    assert_eq!(hits(&mut ui), (3, 0), "대소문자 무시, 덩어리를 넘어");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert_eq!(hits(&mut ui), (3, 1));
    ui.simulate_keystrokes("shift-enter");
    ui.simulate_keystrokes("shift-enter");
    ui.run_until_parked();
    assert_eq!(hits(&mut ui), (3, 2), "앞으로 돌면 끝으로");
    ui.simulate_keystrokes("escape");
    ui.run_until_parked();
    assert!(ui.debug_bounds("page-find-count").is_none());
}
