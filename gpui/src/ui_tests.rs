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
    // 바깥을 누르면 아무것도 바꾸지 않고 닫힌다 — 레일의 빈자리(오늘 단추는 실제 날짜로 옮겨 가서 안 된다)
    click(&mut ui, "vac");
    ui.simulate_click(point(px(34.), px(600.)), Modifiers::none());
    ui.run_until_parked();
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

/// 바깥(다른 편집기)이 고친 것을 감시가 받을 때까지 — FSEvents 는 실제 시간으로 온다
fn let_watch_fire(ui: &mut VisualTestContext) {
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        ui.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        ui.run_until_parked();
    }
}

fn copied_text(ui: &mut VisualTestContext) -> Option<String> {
    click(ui, "note-copy");
    ui.read_from_clipboard().and_then(|c| c.text())
}

#[gpui_kit::test]
fn an_outside_edit_reloads_the_open_note(cx: &mut TestAppContext) {
    let db = seeded("watch-read", &[]);
    vault_note(&db, "A.md", "# A\n\nold\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-A.md");
    ui.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(300));
    vault_note(&db, "A.md", "# A\n\nfrom vim\n");
    vault_note(&db, "B.md", "# B\n");
    let_watch_fire(&mut ui);
    assert_eq!(
        copied_text(&mut ui).as_deref(),
        Some("# A\n\nfrom vim\n"),
        "읽던 노트가 바뀐다"
    );
    assert!(ui.debug_bounds("tree-B.md").is_some(), "트리도 다시 읽는다");
}

#[gpui_kit::test]
fn an_outside_edit_during_unsaved_edits_asks_first(cx: &mut TestAppContext) {
    let db = seeded("watch-edit", &[]);
    vault_note(&db, "A.md", "# A\n");
    let mut ui = open(cx, &db);
    edit_note(&mut ui, "tree-A.md", "# mine\n");
    vault_note(&db, "A.md", "# theirs\n");
    // 창으로 돌아온 것처럼 맞춘다
    ui.update(|_, cx| {
        spike().update(cx, |s, cx| {
            s.sync_from_disk(cx);
            cx.notify();
        })
    });
    ui.run_until_parked();
    assert!(
        ui.debug_bounds("disk-load").is_some(),
        "초안을 덮지 않고 묻는다"
    );
    click(&mut ui, "disk-load");
    ui.update(|_, cx| {
        let d = spike().read(cx).note_draft(cx);
        assert_eq!(
            d.as_deref(),
            Some("# theirs\n"),
            "새 내용으로 편집을 이어 간다"
        );
    });
    assert!(ui.debug_bounds("note-save").is_some());
}

/// 사이드카(<이름>.comments.json)를 노트 옆에 깐다
fn vault_sidecar(db: &PathBuf, rel: &str, json: &str) {
    let note = db.parent().unwrap().join("vault/notes").join(rel);
    let path = note.with_file_name(format!(
        "{}.comments.json",
        note.file_stem().unwrap().to_str().unwrap()
    ));
    std::fs::write(path, json).unwrap();
}

fn sidecar_ids(db: &PathBuf, rel: &str) -> Vec<String> {
    let note = db.parent().unwrap().join("vault/notes").join(rel);
    let path = note.with_file_name(format!(
        "{}.comments.json",
        note.file_stem().unwrap().to_str().unwrap()
    ));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let d: serde_json::Value = serde_json::from_str(&text).unwrap();
    d["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[gpui_kit::test]
fn note_questions_list_in_text_order_and_open_threads(cx: &mut TestAppContext) {
    let db = seeded("cmt-list", &[]);
    vault_note(&db, "Q.md", "# Q\n\nalpha beta gamma\n\nbeta again\n");
    vault_sidecar(
        &db,
        "Q.md",
        r#"{"version":1,"comments":[
  {"id":"c1","anchor":"beta","occurrence":1,"question":"second beta?","answer":"yes","createdAt":3},
  {"id":"c2","anchor":"not in the text","occurrence":0,"question":"lost?","answer":"gone","createdAt":1},
  {"id":"c3","anchor":"alpha","occurrence":0,"question":"why alpha?","answer":"**first**","createdAt":2,
   "followUps":[{"question":"more?","answer":"more","createdAt":4}]}
]}"#,
    );
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-Q.md");
    ui.update(|_, cx| {
        let c = &spike().read(cx).notes.cmts;
        assert_eq!(c.list.len(), 3);
        assert!(c.at.contains_key("c1") && c.at.contains_key("c3"));
        assert!(!c.at.contains_key("c2"), "본문에 없는 앵커");
        // 두 번째 beta = 'beta again' 의 것(앞 덩어리의 beta 보다 뒤)
        assert!(c.at["c1"].2 > c.at["c3"].2);
    });
    ui.update(|_, cx| {
        // 형광펜 범위가 그 덩어리의 그려진 글자에서 'beta again' 의 beta 를 가리킨다
        let s = spike().read(cx);
        let (bi, r, _) = s.notes.cmts.at["c1"].clone();
        let state = crate::notes_view::text_states(&s.notes.blocks)[bi].clone();
        let text = state.read(cx).rendered_text();
        assert_eq!(&text.as_str()[r.clone()], "beta");
        assert!(text.as_str()[r.start..].starts_with("beta again"));
    });
    click(&mut ui, "cmt-trigger");
    // 본문 등장 순서: alpha(c3), beta again(c1), 사라진 것(c2)은 끝
    let y = |ui: &mut VisualTestContext, n: &'static str| bounds(ui, n).origin.y;
    let (y3, y1, y2) = (
        y(&mut ui, "cmt-open-c3"),
        y(&mut ui, "cmt-open-c1"),
        y(&mut ui, "cmt-open-c2"),
    );
    assert!(y3 < y1 && y1 < y2, "{y3:?} {y1:?} {y2:?}");
    click(&mut ui, "cmt-open-c3");
    assert!(ui.debug_bounds("cmt-thread").is_some());
    assert!(ui.debug_bounds("cmt-back").is_some(), "목록에서 왔다");
    ui.update(|_, cx| {
        let c = &spike().read(cx).notes.cmts;
        assert_eq!(
            c.pop,
            Some(crate::note_comments::Pop::View {
                id: "c3".into(),
                from_list: true
            })
        );
        assert!(c.input.is_some(), "이어 묻기 입력칸");
    });
    click(&mut ui, "cmt-back");
    assert!(ui.debug_bounds("cmt-open-c1").is_some(), "다시 목록");
    // 스레드에서 지운다 — 확인 뒤 사이드카에서 빠진다
    click(&mut ui, "cmt-open-c1");
    click(&mut ui, "cmt-delete");
    assert_eq!(sidecar_ids(&db, "Q.md").len(), 3, "확인 전엔 그대로");
    click(&mut ui, "confirm-delete");
    assert_eq!(sidecar_ids(&db, "Q.md"), vec!["c2", "c3"]);
    assert!(
        ui.debug_bounds("cmt-thread").is_none(),
        "지운 스레드는 닫힌다"
    );
    assert!(ui.debug_bounds("cmt-trigger").is_some());
}

/// 가짜 claude — 받은 입력을 옆 파일에 남기고 정해진 답을 봉투로 낸다
fn fake_claude(dir: &std::path::Path, answer: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("fake-claude");
    let log = dir.join("fake-claude.in");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\ncat >> '{}'\nprintf '%s' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"{answer}\",\"session_id\":\"s\"}}'\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// AI 는 진짜 스레드(tokio)에서 돈다 — 답이 올 때까지 실제 시간을 기다린다
fn wait_answer(ui: &mut VisualTestContext) {
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        ui.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        ui.run_until_parked();
        if ui.update(|_, cx| spike().read(cx).notes.cmts.asking.is_none()) {
            return;
        }
    }
    panic!("답이 오지 않았다");
}

#[gpui_kit::test]
fn dragging_over_text_asks_ai_and_keeps_the_thread(cx: &mut TestAppContext) {
    let db = seeded("cmt-ask", &[]);
    vault_note(
        &db,
        "A.md",
        "# A\n\nThe quick brown fox jumps over the lazy dog\n",
    );
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude(&dir, "Because it is quick.");
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        amber_gpui::ai_config::connect_provider(&d, "claude", bin.to_str().unwrap(), None).unwrap();
    }
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-A.md");
    // 첫 덩어리(제목 + 문단)의 문단 줄을 왼쪽에서 가운데까지 끈다
    let b = bounds(&mut ui, "note-block-0");
    let y = b.origin.y + b.size.height - px(14.);
    let from = point(b.origin.x + px(2.), y);
    let to = point(b.origin.x + px(120.), y);
    drag_points(&mut ui, from, to, Modifiers::none());
    let sel = ui.update(|_, cx| spike().read(cx).notes.cmts.sel.clone());
    let sel = sel.expect("선택하면 [질문] 버튼");
    assert!(
        "The quick brown fox jumps over the lazy dog".starts_with(&sel.anchor),
        "앵커는 그려진 글자: {:?}",
        sel.anchor
    );
    assert_eq!(sel.occurrence, 0);
    click(&mut ui, "cmt-fab");
    assert!(ui.debug_bounds("cmt-submit").is_some(), "질문 패널");
    ui.simulate_input("why quick?");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    wait_answer(&mut ui);
    ui.update(|_, cx| {
        let c = &spike().read(cx).notes.cmts;
        assert_eq!(c.error, None);
        assert_eq!(c.list.len(), 1);
        assert_eq!(c.list[0]["answer"], "Because it is quick.");
        assert_eq!(c.list[0]["question"], "why quick?");
        assert_eq!(c.list[0]["anchor"].as_str(), Some(sel.anchor.as_str()));
        assert!(
            matches!(
                &c.pop,
                Some(crate::note_comments::Pop::View {
                    from_list: false,
                    ..
                })
            ),
            "답이 오면 그 스레드"
        );
        assert!(
            c.at.contains_key(c.list[0]["id"].as_str().unwrap()),
            "형광펜"
        );
    });
    assert_eq!(sidecar_ids(&db, "A.md").len(), 1, "사이드카에 남는다");
    // 이어 묻기 — 이전 문답을 문맥으로 싣는다
    ui.simulate_input("and lazy?");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    wait_answer(&mut ui);
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    assert!(
        sent.contains("[이전 문답]\nQ: why quick?\nA: Because it is quick."),
        "{sent}"
    );
    ui.update(|_, cx| {
        let c = &spike().read(cx).notes.cmts;
        assert_eq!(c.list[0]["followUps"][0]["question"], "and lazy?");
    });
}

#[gpui_kit::test]
fn questions_and_find_reach_inside_toggles(cx: &mut TestAppContext) {
    let db = seeded("cmt-toggle", &[]);
    vault_note(
        &db,
        "T.md",
        "# T\n\nouter line\n\n<details>\n<summary>More</summary>\n\ninner secret line\n\n</details>\n",
    );
    vault_sidecar(
        &db,
        "T.md",
        r#"{"version":1,"comments":[
  {"id":"t1","anchor":"inner secret","occurrence":0,"question":"q?","answer":"a","createdAt":1}
]}"#,
    );
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-T.md");
    ui.update(|_, cx| {
        let s = spike().read(cx);
        let (bi, r, _) = s
            .notes
            .cmts
            .at
            .get("t1")
            .cloned()
            .expect("닫힌 토글 안도 찾는다");
        let state = crate::notes_view::text_states(&s.notes.blocks)[bi].clone();
        assert_eq!(&state.read(cx).rendered_text().as_str()[r], "inner secret");
    });
    click(&mut ui, "note-scroll");
    ui.simulate_keystrokes("cmd-f");
    ui.simulate_input("line");
    ui.run_until_parked();
    let n = ui.update(|_, cx| {
        spike()
            .read(cx)
            .notes
            .page_find
            .as_ref()
            .unwrap()
            .hits
            .len()
    });
    assert_eq!(n, 2, "토글 안 글도 찾는다");
}

#[gpui_kit::test]
fn revising_an_answer_replaces_it_in_place(cx: &mut TestAppContext) {
    let db = seeded("cmt-revise", &[]);
    vault_note(&db, "R.md", "# R\n\nalpha beta\n");
    vault_sidecar(
        &db,
        "R.md",
        r#"{"version":1,"comments":[
  {"id":"r1","anchor":"alpha","occurrence":0,"question":"q0","answer":"first answer","createdAt":1,
   "followUps":[{"question":"q1","answer":"second answer","createdAt":2}]}
]}"#,
    );
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude(&dir, "Shorter.");
    {
        let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
        amber_gpui::ai_config::connect_provider(&d, "claude", bin.to_str().unwrap(), None).unwrap();
    }
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-R.md");
    click(&mut ui, "cmt-trigger");
    click(&mut ui, "cmt-open-r1");
    // Esc 로 접힌다
    click(&mut ui, "cmt-revise-0");
    assert!(ui.debug_bounds("cmt-revise-send").is_some());
    ui.simulate_keystrokes("escape");
    ui.run_until_parked();
    assert!(ui.debug_bounds("cmt-revise-send").is_none());
    // 둘째 답을 고친다 — 덧붙이지 않고 그 자리에서
    click(&mut ui, "cmt-revise-1");
    ui.simulate_input("make it shorter");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    wait_answer(&mut ui);
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    assert!(sent.contains("[고쳐 쓸 답변]\nsecond answer"), "{sent}");
    assert!(
        sent.contains("[질문]\nmake it shorter"),
        "지시가 질문 자리에"
    );
    ui.update(|_, cx| {
        let c = &spike().read(cx).notes.cmts;
        assert_eq!(c.error, None);
        assert_eq!(c.list[0]["answer"], "first answer");
        assert_eq!(c.list[0]["followUps"][0]["answer"], "Shorter.");
        assert_eq!(
            c.list[0]["followUps"].as_array().unwrap().len(),
            1,
            "덧붙지 않는다"
        );
        assert!(c.revise.is_none(), "다 고치면 지시 줄이 닫힌다");
    });
    assert_eq!(sidecar_ids(&db, "R.md"), vec!["r1"]);
}

/// 가짜 claude(stream-json) — 받은 입력을 남기고, 입력의 [초안 폴더] 에 절 파일을 써서 실제 파일 모드처럼 끝낸다.
/// files 가 비면 초안 없이 reply 를 본문으로 낸다. delay 초만큼 기다렸다 끝낸다(진행 중을 보려고)
fn fake_claude_stream(
    dir: &std::path::Path,
    files: &[(&str, &str)],
    reply: &str,
    delay: u32,
) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("fake-claude-stream");
    let log = dir.join("fake-claude.in");
    let mut writes = String::new();
    for (name, body) in files {
        let quoted = body.replace('\'', "'\\''");
        writes.push_str(&format!("  printf '%s' '{quoted}' > \"$d/{name}\"\n"));
    }
    let json = |s: &str| serde_json::to_string(s).unwrap().replace('\'', "'\\''");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\ninput=$(cat)\nprintf '%s\\n----\\n' \"$input\" >> '{log}'\n\
d=$(printf '%s\\n' \"$input\" | awk 'f {{ print; exit }} /^\\[초안 폴더\\]$/ {{ f = 1 }}')\n\
if [ -n \"$d\" ] && [ -d \"$d\" ]; then\n  :\n{writes}fi\n\
printf '%s\\n' '{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_start\",\"content_block\":{{\"type\":\"text\"}}}}}}'\n\
printf '%s\\n' '{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":{r}}}}}}}'\n\
sleep {delay}\n\
printf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":{r},\"session_id\":\"s\"}}'\n",
            log = log.display(),
            r = json(reply),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn connect_fake(db: &PathBuf, bin: &std::path::Path) {
    let d = Db::open(db.to_str().unwrap(), Clock::System).unwrap();
    amber_gpui::ai_config::connect_provider(&d, "claude", bin.to_str().unwrap(), None).unwrap();
}

/// 그 노트의 AI 실행이 끝날 때까지(실제 시간)
fn wait_note_ai(ui: &mut VisualTestContext, path: &str) {
    for _ in 0..80 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        ui.executor()
            .advance_clock(std::time::Duration::from_millis(150));
        ui.run_until_parked();
        if !ui.update(|_, cx| spike().read(cx).notes.ai.running(path)) {
            return;
        }
    }
    panic!("AI 가 끝나지 않았다");
}

#[gpui_kit::test]
fn writing_a_note_with_ai_reviews_the_diff_then_applies_a_draft(cx: &mut TestAppContext) {
    let db = seeded("ai-compose", &[]);
    vault_note(&db, "C.md", "# C\n\nold body\n");
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(
        &dir,
        &[
            ("01.md", "# C\n\nnew body"),
            ("02.md", "## 2. More\n\nextra"),
        ],
        "DONE 2",
        0,
    );
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-C.md");
    click(&mut ui, "note-ai");
    assert!(ui.debug_bounds("ai-run").is_some(), "지시 화면");
    ui.simulate_input("rewrite it");
    click(&mut ui, "ai-chip-p:0");
    click(&mut ui, "ai-run");
    wait_note_ai(&mut ui, "C.md");
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    let preset = amber_gpui::i18n::t("notes.ai.preset1");
    assert!(
        sent.contains(&format!("[작성 요청]\nrewrite it\n\n{preset}")),
        "내가 친 말 → 빠른 지시: {sent}"
    );
    assert!(
        sent.contains("[현재 본문 (Markdown)]\n# C\n\nold body"),
        "{sent}"
    );
    ui.update(|_, cx| {
        let r = spike().read(cx).notes.ai.get("C.md").cloned().unwrap();
        assert_eq!(r.phase, crate::note_ai::Phase::Done);
        assert_eq!(
            r.result.as_deref(),
            Some("# C\n\nnew body\n\n## 2. More\n\nextra"),
            "초안 파일을 이름순으로 잇는다"
        );
        assert!(!r.truncated, "DONE 이 왔다");
    });
    // 기존 노트라 변경점부터
    assert!(ui.debug_bounds("ai-diff").is_some());
    click(&mut ui, "ai-tab-source");
    assert!(ui.debug_bounds("ai-diff").is_none());
    click(&mut ui, "ai-apply");
    ui.update(|_, cx| {
        let s = spike().read(cx);
        assert!(s.notes.ai_modal.is_none(), "적용하면 닫힌다");
        assert!(s.notes.ai.get("C.md").is_none(), "기다리던 결과는 끝");
        assert_eq!(
            s.note_draft(cx).as_deref(),
            Some("# C\n\nnew body\n\n## 2. More\n\nextra"),
            "편집 초안으로"
        );
        assert!(s.notes.edit.as_ref().unwrap().dirty, "저장은 사용자가 한다");
    });
    assert_eq!(read_note(&db, "C.md"), "# C\n\nold body\n", "파일은 그대로");
}

#[gpui_kit::test]
fn closing_keeps_the_ai_running_with_a_banner_and_stop_ends_it(cx: &mut TestAppContext) {
    let db = seeded("ai-bg", &[]);
    vault_note(&db, "B.md", "# B\n");
    vault_note(&db, "Other.md", "# O\n");
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(&dir, &[], "# B\n\nslow", 30);
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-B.md");
    click(&mut ui, "note-ai");
    ui.simulate_input("write");
    click(&mut ui, "ai-run");
    assert!(ui.debug_bounds("ai-stop").is_some(), "쓰는 중");
    click(&mut ui, "ai-keep");
    assert!(ui.debug_bounds("ai-stop").is_none(), "닫기는 숨기기");
    assert!(ui.debug_bounds("ai-banner").is_some(), "노트 위 배너");
    // 다른 노트로 가도 계속 돈다 — 배너는 그 노트에만
    click(&mut ui, "tree-Other.md");
    assert!(ui.debug_bounds("ai-banner").is_none());
    assert!(ui.update(|_, cx| spike().read(cx).notes.ai.running("B.md")));
    click(&mut ui, "tree-B.md");
    click(&mut ui, "ai-bg-stop");
    ui.update(|_, cx| {
        assert!(
            spike().read(cx).notes.ai.get("B.md").is_none(),
            "받은 결과가 없으니 실행이 사라진다"
        );
    });
    assert!(ui.debug_bounds("ai-banner").is_none());
}

#[gpui_kit::test]
fn a_run_finished_on_another_tab_lights_the_rail(cx: &mut TestAppContext) {
    let db = seeded("ai-rail", &[]);
    vault_note(&db, "R.md", "");
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(&dir, &[], "# R\n\nwritten", 1);
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-R.md");
    click(&mut ui, "note-ai");
    ui.simulate_input("write");
    click(&mut ui, "ai-run");
    click(&mut ui, "ai-keep");
    click(&mut ui, "app.rail.todo");
    wait_note_ai(&mut ui, "R.md");
    ui.update(|_, cx| {
        let s = spike().read(cx);
        let r = s.notes.ai.get("R.md").cloned().unwrap();
        assert_eq!(
            (r.phase, r.error.clone()),
            (crate::note_ai::Phase::Done, None)
        );
        assert!(s.notes.rail_done, "완료 점이 켜졌다");
    });
    assert!(ui.debug_bounds("rail-done").is_some(), "다른 탭에서 끝났다");
    click(&mut ui, "app.rail.notes");
    assert!(
        ui.debug_bounds("rail-done").is_none(),
        "그 탭에 들어가면 꺼진다"
    );
    // 빈 노트라 변경점 없이 미리보기, 배너는 '초안 준비됨'
    assert!(ui.debug_bounds("ai-banner").is_some());
    click(&mut ui, "ai-bg-review");
    assert!(ui.debug_bounds("ai-diff").is_none());
    assert!(ui.debug_bounds("ai-preview").is_some());
}

#[gpui_kit::test]
fn editing_one_section_with_ai_splices_it_back(cx: &mut TestAppContext) {
    let db = seeded("span-sec", &[]);
    let body = "# 1. A\n\nold a\n\n## 2. B\n\nold b\n\n## 3. C\n\nold c\n";
    vault_note(&db, "S.md", body);
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(&dir, &[], "## 2. B\n\nnew b", 0);
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-S.md");
    click(&mut ui, "note-span-ai");
    assert!(ui.debug_bounds("span-pick-1").is_some(), "절 고르기부터");
    click(&mut ui, "span-pick-1");
    click(&mut ui, "span-next");
    ui.simulate_input("make it new");
    ui.simulate_keystrokes("cmd-enter");
    ui.run_until_parked();
    wait_note_ai(&mut ui, "S.md");
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    assert!(sent.contains("[대상 종류]\n절 전체"), "{sent}");
    assert!(sent.contains("[선택한 부분]\n## 2. B\n\nold b"), "{sent}");
    let want = "# 1. A\n\nold a\n\n## 2. B\n\nnew b\n\n## 3. C\n\nold c\n";
    ui.update(|_, cx| {
        let r = spike().read(cx).notes.ai.get("S.md").cloned().unwrap();
        assert_eq!(r.phase, crate::note_ai::Phase::Done);
        assert_eq!(r.result.as_deref(), Some(want), "절 사이 빈 줄은 그대로");
    });
    assert!(ui.debug_bounds("ai-diff").is_some(), "조각별 변경점");
    click(&mut ui, "span-apply");
    ui.update(|_, cx| {
        let s = spike().read(cx);
        assert!(s.notes.span_modal.is_none());
        assert_eq!(s.note_draft(cx).as_deref(), Some(want));
    });
}

#[gpui_kit::test]
fn editing_the_selected_source_with_ai_uses_its_bytes(cx: &mut TestAppContext) {
    let db = seeded("span-sel", &[]);
    vault_note(&db, "E.md", "# E\n\nfix this\n");
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(&dir, &[], "# E\n\nfixed", 0);
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-E.md");
    click(&mut ui, "note-edit");
    ui.update(|_, cx| assert!(!spike().read(cx).notes.edit.as_ref().unwrap().has_sel));
    ui.simulate_keystrokes("cmd-a");
    ui.run_until_parked();
    ui.update(|_, cx| {
        assert!(
            spike().read(cx).notes.edit.as_ref().unwrap().has_sel,
            "고르면 단추가 켜진다"
        )
    });
    click(&mut ui, "note-span-ai");
    assert!(
        ui.debug_bounds("span-pick-0").is_none(),
        "고른 구간이면 바로 지시"
    );
    ui.simulate_input("fix it");
    click(&mut ui, "span-run");
    wait_note_ai(&mut ui, "E.md");
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    assert!(sent.contains("[대상 종류]\n선택 영역"), "{sent}");
    ui.update(|_, cx| {
        let r = spike().read(cx).notes.ai.get("E.md").cloned().unwrap();
        assert_eq!(
            r.result.as_deref(),
            Some("# E\n\nfixed\n"),
            "끝 개행은 원래 조각의 것"
        );
    });
}

#[gpui_kit::test]
fn separate_sections_run_one_after_another(cx: &mut TestAppContext) {
    let db = seeded("span-runs", &[]);
    vault_note(&db, "M.md", "# A\n\na\n\n# B\n\nb\n\n# C\n\nc\n");
    let dir = db.parent().unwrap().to_path_buf();
    let bin = fake_claude_stream(&dir, &[], "# X\n\nx", 0);
    connect_fake(&db, &bin);
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.notes");
    click(&mut ui, "tree-M.md");
    click(&mut ui, "note-span-ai");
    click(&mut ui, "span-pick-0");
    click(&mut ui, "span-pick-2");
    click(&mut ui, "span-next");
    ui.update(|_, cx| {
        let m = spike().read(cx);
        let runs = &m.notes.span_modal.as_ref().unwrap().runs;
        assert_eq!(runs.len(), 2, "떨어진 절은 따로");
    });
    ui.simulate_input("redo");
    click(&mut ui, "span-run");
    wait_note_ai(&mut ui, "M.md");
    ui.update(|_, cx| {
        let r = spike().read(cx).notes.ai.get("M.md").cloned().unwrap();
        assert_eq!(r.span_results.len(), 2);
        assert_eq!(
            r.result.as_deref(),
            Some("# X\n\nx\n\n# B\n\nb\n\n# X\n\nx\n"),
            "뒤에서부터 되끼운다"
        );
    });
    let sent = std::fs::read_to_string(dir.join("fake-claude.in")).unwrap();
    assert_eq!(sent.matches("[수정 지시]").count(), 2, "묶음마다 한 번");
}

/// 다이어그램 보관함(데이터 폴더의 vault/diagrams)에 파일을 깐다
fn vault_diagram(db: &PathBuf, rel: &str, body: &str) {
    let path = db.parent().unwrap().join("vault/diagrams").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn diagram_has(db: &PathBuf, rel: &str) -> bool {
    db.parent()
        .unwrap()
        .join("vault/diagrams")
        .join(rel)
        .exists()
}

#[gpui_kit::test]
fn the_diagrams_tree_opens_creates_renames_and_moves(cx: &mut TestAppContext) {
    let db = seeded("dgm-tree", &[]);
    vault_diagram(
        &db,
        "ERD/order.mmd",
        "erDiagram\n  USER ||--o{ ORDER : places\n",
    );
    vault_diagram(&db, "flow.mmd", "flowchart LR\n  A --> B\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.diagrams");
    assert!(ui.debug_bounds("dtree-flow.mmd").is_some());
    assert!(
        ui.debug_bounds("tree-flow.mmd").is_none(),
        "노트 트리가 아니다"
    );
    click(&mut ui, "dtree-ERD");
    click(&mut ui, "dtree-ERD/order.mmd");
    ui.update(|_, cx| {
        let d = &spike().read(cx).diagrams;
        assert_eq!(d.selected.as_deref(), Some("ERD/order.mmd"));
        assert!(matches!(d.figure, Some(Ok(_))), "그렸다");
    });
    assert!(ui.debug_bounds("dgm-figure").is_some());
    // 새 다이어그램 — 여러 단 경로, 확장자는 보관함이 붙인다
    click(&mut ui, "dnotes-new-note");
    ui.simulate_input("New/one");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert!(diagram_has(&db, "ERD/New/one.mmd"), "고른 폴더(ERD) 아래에");
    ui.update(|_, cx| {
        assert_eq!(
            spike().read(cx).diagrams.selected.as_deref(),
            Some("ERD/New/one.mmd")
        );
    });
    // 이름 바꾸기 — 칸에는 확장자 없는 이름
    click(&mut ui, "drow-rename-flow.mmd");
    ui.simulate_keystrokes("cmd-a");
    ui.simulate_input("flow2");
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    assert!(diagram_has(&db, "flow2.mmd") && !diagram_has(&db, "flow.mmd"));
    // 끌어서 폴더로
    let to = bounds(&mut ui, "dtree-ERD").center();
    drag(&mut ui, "dtree-flow2.mmd", to);
    assert!(diagram_has(&db, "ERD/flow2.mmd"));
    assert!(
        !db.parent().unwrap().join("vault/notes/ERD").exists(),
        "노트 보관함은 그대로"
    );
}

#[gpui_kit::test]
fn deleting_a_diagram_asks_then_trashes_it(cx: &mut TestAppContext) {
    let db = seeded("dgm-delete", &[]);
    vault_diagram(&db, "x.mmd", "flowchart LR\n  A --> B\n");
    let mut ui = open_fixed(cx, &db);
    click(&mut ui, "app.rail.diagrams");
    click(&mut ui, "dtree-x.mmd");
    click(&mut ui, "dgm-delete");
    click(&mut ui, "confirm-delete");
    let dir = db.parent().unwrap();
    assert!(!diagram_has(&db, "x.mmd"));
    assert!(
        dir.join(".trash/1/x.mmd").exists(),
        "지우지 않고 휴지통으로"
    );
    assert!(
        ui.debug_bounds("dgm-delete").is_none(),
        "열려 있던 파일은 닫힌다"
    );
}

#[gpui_kit::test]
fn diagrams_have_their_own_find_and_work_folder(cx: &mut TestAppContext) {
    let db = seeded("dgm-root", &[]);
    vault_diagram(&db, "Deep/Deeper/target.mmd", "flowchart LR\n  A --> B\n");
    vault_note(&db, "note.md", "# n\n");
    let mut ui = open(cx, &db);
    click(&mut ui, "app.rail.diagrams");
    // 트리 판을 누르면 ⌘F 가 그 트리의 찾기 — 접힌 폴더까지
    click(&mut ui, "dnotes-tree");
    ui.simulate_keystrokes("cmd-f");
    ui.simulate_input("targ");
    ui.run_until_parked();
    assert!(ui.debug_bounds("dtree-find-count").is_some());
    ui.simulate_keystrokes("enter");
    ui.run_until_parked();
    ui.update(|_, cx| {
        assert_eq!(
            spike().read(cx).diagrams.selected.as_deref(),
            Some("Deep/Deeper/target.mmd")
        );
    });
    // 작업 폴더는 탭마다 — 다이어그램만 다른 폴더로
    let other = db.parent().unwrap().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("there.mmd"), "flowchart LR\n  X --> Y\n").unwrap();
    ui.update(|_, cx| {
        spike().update(cx, |s, cx| {
            s.apply_root(
                crate::vault_pane::Which::Diagrams,
                Some(other.to_string_lossy().into_owned()),
            );
            cx.notify();
        })
    });
    ui.run_until_parked();
    assert!(ui.debug_bounds("dtree-there.mmd").is_some());
    click(&mut ui, "app.rail.notes");
    assert!(
        ui.debug_bounds("tree-note.md").is_some(),
        "노트 보관함은 그대로"
    );
}
