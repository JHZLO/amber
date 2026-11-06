// GPUI 시험판: 지금 앱의 셸(왼쪽 레일 + 작업 영역) 위에 할 일 하루치(DB 사본)와 노트 하나(글, svg, mermaid).
// 확인하려는 것 — 한글 입력(IME), 그림이 지금 앱과 같게 나오는지, 시작 시간과 메모리. 탭 내용은 하나씩 옮긴다.
//
//   cargo run --release -- --db <amber.db 사본> [--note <보관함 기준 노트 경로>] [--date YYYY-MM-DD] [--jump 2-3]

mod ai;
mod assets;
mod calendar;
mod day_card;
mod diagram_ai;
mod diagram_canvas;
mod diagrams_view;
mod dialog;
mod drawer;
mod erd_details;
mod erd_overview;
mod erd_view;
mod figure;
mod highlight;
mod markdown;
mod menu;
mod mermaid_zoom;
mod note;
mod note_ai;
mod note_ai_view;
mod note_comments;
mod note_edit;
mod note_find;
mod note_names;
mod note_page_find;
mod note_span_ai;
mod note_watch;
mod notes_view;
mod pane;
mod raster;
mod root_picker;
mod row_drag;
#[cfg(feature = "shots")]
mod shots;
mod spinner;
mod theme;
mod tip;
mod tree_drag;
mod tt_card;
mod tt_drag;
#[cfg(test)]
mod ui_tests;
mod vault_pane;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amber_gpui::db::{Clock, Db, Row};
use amber_gpui::todos;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;
use amber_gpui::i18n::{self, t};

/// 왼쪽 레일의 작업공간(App.tsx 의 RAIL) — 이름은 상단바 제목에도 쓴다
/// 마지막으로 연 탭은 지금 앱처럼 til.section 에 남는다
#[derive(Clone, Copy, PartialEq)]
enum Section {
    Todo,
    Til,
    Notes,
    Erd,
    Diagrams,
}

const RAIL: [(Section, &str, &str, &str); 5] = [
    (Section::Todo, "todo", "app.rail.todo", "calendar-check"),
    (Section::Til, "til", "app.rail.til", "layers"),
    (Section::Notes, "notes", "app.rail.notes", "book"),
    (Section::Erd, "erd", "app.rail.erd", "database"),
    (
        Section::Diagrams,
        "diagrams",
        "app.rail.diagrams",
        "workflow",
    ),
];

struct Spike {
    db: Db,
    date: String,
    /// 고른 날의 목록(listTodos)과 그 날 이전의 밀린 항목(listOverdueOpen), 서랍(언젠가)에 든 수
    todos: Vec<Row>,
    /// 주 단위일 때 그 주의 '주 할 일'(scope = week)
    week_todos: Vec<Row>,
    overdue: Vec<Row>,
    overdue_open: bool,
    parked_n: usize,
    /// 서랍: 열렸는가(amber.todo.parked-open), 탭, 내려놓은 행과 그 조상(묶음 이름을 단다)
    parked_open: bool,
    drawer_tab: drawer::Tab,
    parked: Vec<Row>,
    parked_anc: Vec<Row>,
    /// 고치는 중인 줄, 하위를 적는 중인 부모, 지우기 전에 묻는 것
    editing: Option<(i64, Entity<gpui_kit::component::input::TextareaState>)>,
    adding_child: Option<(i64, Entity<InputState>)>,
    confirm: Option<day_card::Confirm>,
    input: Entity<InputState>,
    /// 노트 탭: 트리, 펼친 폴더, 고른 노트와 그 본문 덩어리, 판 폭
    notes: notes_view::NotesState,
    scroll: ScrollHandle,
    section: Section,
    /// 앱 표식(책상 스탠드). 스탠드 색이 테마를 따르므로 테마별로 굽는다
    mark: Option<(bool, figure::Figure)>,
    /// 할 일 탭의 달력(보고 있는 달, 단위, 날짜별 진행, 휴가)과 달력 판 폭(amber.todo.cal-width)
    cal: calendar::CalendarData,
    cal_w: f32,
    /// 타임테이블: 뷰(amber.todo.tt-view), 그 범위의 블록, 스크롤, 스크롤을 맞춘 범위
    tt_view: tt_card::View,
    tt_blocks: Vec<Row>,
    tt_scroll: ScrollHandle,
    tt_scrolled: Option<String>,
    /// 블록 조작: 그리드를 누른 자리(⌥ 여부), 끄는 중인 미리보기, 그리드의 자리, 제목을 고치는 블록
    tt_press: Option<(Point<Pixels>, bool)>,
    tt_drag: Option<tt_drag::TtDrag>,
    tt_grid: tt_drag::GridBounds,
    tt_editing: Option<(i64, Entity<InputState>)>,
    /// 휴가 고르기 판이 열렸는가, 트리거의 자리
    vac_open: bool,
    /// 판 폭 끌기: 손잡이를 누른 x, 끄는 중인 것
    pane_press: Option<Pixels>,
    pane_drag: Option<pane::PaneDrag>,
    vac_anchor: Rc<Cell<Bounds<Pixels>>>,
    /// 줄 끌기: 손잡이를 누른 자리, 끄는 중인 것, 줄마다 잰 자리와 목록 판의 자리
    grip_down: Option<Point<Pixels>>,
    row_drag: Option<row_drag::RowDrag>,
    row_bounds: row_drag::RowBounds,
    list_bounds: row_drag::ListBounds,
    /// 창 단축키(⌥⌘C 등)를 받는 뿌리 포커스 — 아무것도 포커스가 없으면 키가 창 뿌리에서 멈춘다
    focus: FocusHandle,
    /// 노트 트리 판의 포커스 — 여기 있을 때의 ⌘F 는 트리 찾기
    tree_focus: FocusHandle,
    /// 다이어그램 탭 트리의 포커스(⌘F 가 트리 찾기로)
    dtree_focus: FocusHandle,
    diagrams: diagrams_view::DiagramsState,
    /// ERD 탭 — 연결, 스냅샷, 캔버스(erd_view.rs)
    erd: erd_view::ErdState,
    /// mermaid 그림 확대 뷰어 — 노트 그림, 다이어그램 AI 미리보기에서 연다
    mmd_zoom: Option<mermaid_zoom::MmdZoom>,
    started: Instant,
    first_frame: bool,
}

impl Spike {
    fn new(
        db: Db,
        date: String,
        open: Option<String>,
        jump: Option<String>,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("todos.quick.placeholder")));
        // 지금 앱의 기본값(system)처럼 macOS 화면 모드를 따라간다
        cx.observe_window_appearance(window, |this: &mut Spike, window, cx| {
            let pref = amber_gpui::prefs::get(&this.db, "til.theme");
            theme::apply(pref.as_deref(), window.appearance(), cx);
            cx.notify();
        })
        .detach();
        // 창으로 돌아오면(Finder, 다른 편집기에서 고치고 복귀) 노트 탭일 때 트리와 열린 노트를 맞춘다 —
        // 파일 이벤트가 주력이고 이건 안전망이다(잠자기나 감시 실패로 이벤트를 놓쳐도 돌아오는 순간 따라잡는다)
        cx.observe_window_activation(window, |this: &mut Spike, window, cx| {
            if window.is_window_active() && this.section == Section::Notes {
                this.reload_notes();
                this.sync_from_disk(cx);
                cx.notify();
            }
            // 다이어그램 탭은 감시 없이 창으로 돌아올 때 트리만 다시 읽는다(DiagramsView 의 focus)
            if window.is_window_active() && this.section == Section::Diagrams {
                this.reload_tree(vault_pane::Which::Diagrams);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    let text = input.read(cx).value().trim().to_string();
                    if text.is_empty() {
                        return;
                    }
                    let (due, scope) = this.add_target();
                    if let Err(e) = todos::create_todo(&this.db, &text, &due, None, scope) {
                        eprintln!("createTodo: {e}");
                    }
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                    this.reload();
                    this.reload_calendar();
                    cx.notify();
                }
            },
        )
        .detach();

        let section = amber_gpui::prefs::get(&db, "til.section")
            .and_then(|id| RAIL.iter().find(|r| r.1 == id))
            .map_or(Section::Todo, |r| r.0);
        let cal = calendar::CalendarData {
            cursor: amber_gpui::dates::month_of(&date).expect("--date YYYY-MM-DD"),
            unit: match amber_gpui::prefs::get(&db, "amber.todo.unit").as_deref() {
                Some("week") => calendar::Unit::Week,
                _ => calendar::Unit::Day,
            },
            level: calendar::Level::Day,
            counts: HashMap::new(),
            week_counts: HashMap::new(),
            vacations: HashMap::new(),
        };
        // 지금 앱의 CAL_W_MIN/MAX/DEFAULT
        let cal_w = amber_gpui::prefs::get(&db, "amber.todo.cal-width")
            .and_then(|w| w.parse::<f32>().ok())
            .map_or(460., |w| w.clamp(260., 780.));
        let parked_open =
            amber_gpui::prefs::get(&db, "amber.todo.parked-open").as_deref() == Some("1");
        let tt_view = tt_card::View::from_id(
            &amber_gpui::prefs::get(&db, "amber.todo.tt-view").unwrap_or_default(),
        );
        // 지금 앱 usePaneResize: 범위 밖 값은 기본값으로
        // 배치 엔진은 앱 전체에 하나 — 노트 그림도 이 엔진으로 그린다
        amber_gpui::mermaid::set_layout(
            &amber_gpui::prefs::get(&db, "amber.diagram.layout").unwrap_or_default(),
        );
        let list_w = |w: vault_pane::Which| {
            let p = w.pane();
            amber_gpui::prefs::get(&db, p.key)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| (p.min..=p.max).contains(v))
                .unwrap_or(notes_view::LIST_DEFAULT)
        };
        let (notes_w, diagrams_w) = (
            list_w(vault_pane::Which::Notes),
            list_w(vault_pane::Which::Diagrams),
        );
        let erd_w = amber_gpui::prefs::get(&db, erd_view::LIST_PANE.key)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| (erd_view::LIST_PANE.min..=erd_view::LIST_PANE.max).contains(v))
            .unwrap_or(notes_view::LIST_DEFAULT);
        let erd_panel = erd_view::panel_width(&db);
        let mut this = Spike {
            db,
            date,
            todos: Vec::new(),
            week_todos: Vec::new(),
            overdue: Vec::new(),
            overdue_open: false,
            parked_n: 0,
            parked_open,
            drawer_tab: drawer::Tab::Suggest,
            parked: Vec::new(),
            parked_anc: Vec::new(),
            editing: None,
            adding_child: None,
            confirm: None,
            input,
            notes: notes_view::NotesState {
                ui: vault_pane::TreeUi {
                    width: notes_w,
                    ..Default::default()
                },
                ..Default::default()
            },
            scroll: ScrollHandle::new(),
            section,
            mark: None,
            cal,
            cal_w,
            tt_view,
            tt_blocks: Vec::new(),
            tt_scroll: ScrollHandle::new(),
            tt_scrolled: None,
            tt_press: None,
            tt_drag: None,
            tt_grid: Default::default(),
            tt_editing: None,
            vac_open: false,
            pane_press: None,
            pane_drag: None,
            vac_anchor: Default::default(),
            grip_down: None,
            row_drag: None,
            row_bounds: Default::default(),
            list_bounds: Default::default(),
            focus: cx.focus_handle(),
            tree_focus: cx.focus_handle(),
            dtree_focus: cx.focus_handle(),
            diagrams: diagrams_view::DiagramsState {
                ui: vault_pane::TreeUi {
                    width: diagrams_w,
                    ..Default::default()
                },
                ..Default::default()
            },
            mmd_zoom: None,
            erd: {
                let mut e = erd_view::ErdState {
                    width: erd_w,
                    ..Default::default()
                };
                e.canvas.panel_w = erd_panel;
                e
            },
            started,
            first_frame: false,
        };
        this.focus.focus(window, cx);
        this.reload();
        this.reload_calendar();
        this.reload_blocks();
        if this.section == Section::Notes || open.is_some() {
            this.reload_notes();
        }
        if this.section == Section::Diagrams {
            this.reload_tree(vault_pane::Which::Diagrams);
        }
        if let Some(rel) = open {
            this.open_note(&rel, cx);
        }
        if this.cal.unit == calendar::Unit::Week {
            this.input.update(cx, |s, cx| {
                s.set_placeholder(t("todos.week.add"), window, cx)
            });
        }
        // --jump: 절 링크를 누른 것처럼 그 절로 간다. 클릭을 못 보내는 환경에서 확인하려고 둔다 —
        // 글이 다 그려진 뒤에 가야 해서 잠깐 기다린다
        if let Some(sec) = jump {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let _ = this.update(cx, |s, cx| {
                    s.jump(&sec);
                    cx.notify();
                });
            })
            .detach();
        }
        this
    }

    /// listTodos 의 평평한 줄을 화면 순서(부모 아래 자식)로 편다. 부모가 이 날짜에 없는 자식은 맨 위로 — todoTree.visibleRoots
    /// 고른 날의 목록, 밀린 항목(그 날이 오늘 이전일 때만 — 앞날은 아직 밀린 게 아니다), 서랍 수
    fn reload(&mut self) {
        let log = |what: &str, e: rusqlite::Error| eprintln!("{what}: {e}");
        self.todos = todos::list_todos(&self.db, &self.date).unwrap_or_else(|e| {
            log("listTodos", e);
            Vec::new()
        });
        self.week_todos = if self.cal.unit == calendar::Unit::Week {
            let ws = amber_gpui::dates::week_start_of(&self.date);
            todos::list_week_todos(&self.db, &ws).unwrap_or_default()
        } else {
            Vec::new()
        };
        let today = amber_gpui::dates::today_str(chrono::Local::now().timestamp_millis());
        self.overdue = if self.date <= today {
            todos::list_overdue_open(&self.db, &self.date).unwrap_or_else(|e| {
                log("listOverdueOpen", e);
                Vec::new()
            })
        } else {
            Vec::new()
        };
        self.parked = todos::list_parked(&self.db).unwrap_or_default();
        self.parked_anc = todos::list_parked_ancestors(&self.db).unwrap_or_default();
        self.parked_n = self.parked.len();
    }

    /// 밀린 항목을 고른 날로 가져온다. 끝내는 게 아니라 옮기는 것이라 완료 재계산을 하지 않는다
    fn move_here(&mut self, ids: &[i64]) {
        if let Err(e) = todos::move_todos(&self.db, ids, &self.date) {
            eprintln!("moveTodos: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    fn toggle(&mut self, id: i64, parent_id: Option<i64>, done: bool) {
        let r = todos::set_subtree_done(&self.db, id, if done { 1 } else { 0 })
            .and_then(|_| todos::recompute_chain_from(&self.db, parent_id));
        if let Err(e) = r {
            eprintln!("toggle: {e}");
        }
        self.reload();
        self.reload_calendar();
    }

    /// 날짜를 고른다 — 다른 달이면 달력도 그 달로
    fn go_date(&mut self, date: String) {
        if let Some(m) = amber_gpui::dates::month_of(&date) {
            self.cal.cursor = m;
        }
        // 날짜가 바뀌면 일 판으로 — 월/연 판을 띄운 채 뒤에서 날짜만 바뀌면 뭘 보는지 어긋난다
        self.cal.level = calendar::Level::Day;
        self.date = date;
        self.reload();
        self.reload_calendar();
        self.reload_blocks();
    }

    fn set_unit(&mut self, unit: calendar::Unit, window: &mut Window, cx: &mut Context<Self>) {
        self.cal.unit = unit;
        amber_gpui::prefs::set(&self.db, "amber.todo.unit", unit.id());
        let hint = t(if unit == calendar::Unit::Week {
            "todos.week.add"
        } else {
            "todos.quick.placeholder"
        });
        self.input
            .update(cx, |s, cx| s.set_placeholder(hint, window, cx));
        self.reload();
        cx.notify();
    }

    /// 보고 있는 달 격자 범위의 날짜별 진행(목록 아래 요약과 같은 dayProgress 규칙)과 휴가
    fn reload_calendar(&mut self) {
        use amber_gpui::{dates, tree, vacations};
        let grid = dates::month_grid_dates(self.cal.cursor.year, self.cal.cursor.month, 0);
        let (Some(from), Some(to)) = (grid.first(), grid.last()) else {
            return;
        };
        let rows = todos::list_range_rows(&self.db, from, to).unwrap_or_default();
        let mut by_day: HashMap<String, Vec<Row>> = HashMap::new();
        for r in rows {
            let day = r
                .get("day")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            by_day.entry(day).or_default().push(r);
        }
        self.cal.counts = by_day
            .into_iter()
            .map(|(day, list)| (day, tree::day_progress(&list)))
            .filter(|(_, (_, total))| *total > 0)
            .collect();
        // 주 표식은 그 주 첫 칸에만 찍으므로 주 시작일 키로 따로 센다
        self.cal.week_counts = todos::list_week_counts(
            &self.db,
            &dates::week_start_of(from),
            &dates::week_start_of(to),
        )
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            let day = r.get("due_date")?.as_str()?.to_string();
            let n = |k: &str| r.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            Some((day, (n("done"), n("total"))))
        })
        .collect();
        self.cal.vacations = vacations::list_vacations(&self.db, from, to)
            .map(|m| {
                m.into_iter()
                    .map(|(d, k)| (d, k.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.first_frame {
            self.first_frame = true;
            let started = self.started;
            window.on_next_frame(move |_, _| {
                eprintln!("첫 화면까지 {} ms", started.elapsed().as_millis())
            });
        }
        let k = Tokens::get(cx);
        let fg = k.text;
        // 창 밖에서 놓으면 GPUI 는 놓기(on_drop)를 건너뛰고 끌기를 취소한다. 지금 앱은 어디서 놓든
        // 놓은 것이다(window 의 mouseup) — 끌기가 끝났는데 상태가 남아 있으면 여기서 마저 놓는다
        // 노트 탭이면 보관함을 감시한다(작업 폴더가 바뀌면 다시) — 밖에서 고친 노트를 다시 열지 않아도 반영
        if self.section == Section::Notes {
            self.watch_notes_root(cx);
        }
        // 디스크 내용으로 바꿀 초안 — 입력칸 값은 창이 있어야 바뀐다
        if let Some(d) = self.notes.pending_draft.take()
            && let Some(e) = self.notes.edit.as_ref()
        {
            let state = e.state.clone();
            state.update(cx, |s, cx| s.set_value(d.clone(), window, cx));
            let text = hex(k.text);
            let (preview, _) = notes_view::build_blocks(&d, &text, cx);
            if let Some(e) = self.notes.edit.as_mut() {
                e.preview = preview;
                e.dirty = false;
            }
        }
        if self.section == Section::Diagrams {
            self.prepare_diagram_canvas(cx);
        }
        if self.section == Section::Erd {
            self.prepare_erd(window, cx);
        }
        // 질문/후속 입력칸, AI 작성 입력칸과 적용한 초안 — 창이 있어야 만들어진다
        self.apply_cmt_rebuild(window, cx);
        self.apply_ai_pending(window, cx);
        self.apply_span_pending(window, cx);
        self.prepare_diagram_ai(window, cx);
        // 노트 탭을 보고 있으면 완료 점은 꺼진다(본 알림을 남겨 둘 이유가 없다)
        if self.section == Section::Notes {
            self.notes.rail_done = false;
        }
        if !cx.has_active_drag() {
            self.diagrams.canvas.grab = None;
            if self.row_drag.is_some() {
                self.drop_row();
            }
            if self.tt_drag.is_some() {
                self.tt_drop(window, cx);
            }
            if self.pane_drag.is_some() {
                self.pane_drop();
            }
            if let Some(w) = self.dragging_tree() {
                self.tree_drop(w);
            }
        }
        // 판 폭은 희망 폭 — 창이 좁으면 오른쪽 몫(340px)을 남기고 줄인다. 몸통은 레일을 뺀 전부다
        // (지금 앱의 .todo-body 처럼 서랍 칸도 몸통 안이다)
        let parked = self.parked_open && self.cal.unit == calendar::Unit::Day;
        let body_w = f32::from(window.viewport_size().width) - 68.;
        let cal_w = pane::effective(self.cal_w, CAL_PANE.min, body_w);
        let body = match self.section {
            // .todo-body: 달력 판(폭 cal_w, 안쪽 여백 0 4 8 8, 카드 위 8) | 하루 판
            Section::Todo => div()
                .size_full()
                .flex()
                .child(
                    div()
                        .relative()
                        .w(px(cal_w))
                        .flex_shrink_0()
                        .h_full()
                        .pl(px(8.))
                        .pr(px(4.))
                        .pb(px(8.))
                        .pt(px(8.))
                        .flex()
                        .flex_col()
                        .child(self.calendar(&k, cal_w - 8. - 4. - 2. - 24., cx))
                        .child(self.tt_card(&k, cal_w - 8. - 4. - 2., cx))
                        .child(self.pane_resizer(CAL_PANE, cal_w, body_w, &k, cx)),
                )
                .child(
                    // .detail: 바깥 여백 8 8 8 4(판의 반 칸)
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .pt(px(8.))
                        .pr(px(8.))
                        .pb(px(8.))
                        .pl(px(4.))
                        .flex()
                        .flex_col()
                        .child(self.day_card(&k, cx)),
                )
                .when(parked, |d| d.child(self.drawer(&k, cx)))
                .into_any_element(),
            Section::Notes => self.notes_body(body_w, &k, cx).into_any_element(),
            Section::Diagrams => self.diagrams_body(body_w, &k, cx).into_any_element(),
            Section::Erd => self.erd_body(body_w, &k, window, cx).into_any_element(),
            // 아직 옮기지 않은 탭 — 체크리스트(D)에서 하나씩
            _ => div().into_any_element(),
        };

        // AI 작성 모달(실행 상태는 스토어, 이건 보기만)
        let (dgm_full, dgm_prompt, dgm_ai) = if self.section == Section::Diagrams {
            (
                self.diagram_fullscreen(&k, cx),
                self.diagram_prompt(&k, cx),
                self.diagram_ai_view(&k, cx),
            )
        } else {
            (None, None, None)
        };
        let (ai_modal, span_modal) = if self.section == Section::Notes {
            (self.ai_modal_view(&k, cx), self.span_modal_view(&k, cx))
        } else {
            (None, None)
        };
        let confirm = self.confirm.as_ref().map(|c| {
            let (title, body, name) = match c {
                day_card::Confirm::Delete { name, count, .. } => (
                    t("todos.delete.title"),
                    if *count > 0 {
                        i18n::t_with(
                            "todos.delete.confirm",
                            &[("name", "{name}"), ("n", &count.to_string())],
                        )
                    } else {
                        i18n::t_with("todos.delete.confirmOne", &[("name", "{name}")])
                    },
                    name.clone(),
                ),
                day_card::Confirm::Record { name, .. } => (
                    t("todos.record.deleteTitle"),
                    t("todos.record.deleteConfirm"),
                    name.clone(),
                ),
                day_card::Confirm::Parked { name, .. } => (
                    t("todos.parked.deleteTitle"),
                    i18n::t_with("todos.parked.deleteConfirm", &[("name", "{name}")]),
                    name.clone(),
                ),
                day_card::Confirm::Note { name, is_dir, .. } => (
                    t(if *is_dir {
                        "notes.delete.folderTitle"
                    } else {
                        "notes.delete.noteTitle"
                    }),
                    t(if *is_dir {
                        "notes.delete.confirmFolder"
                    } else {
                        "notes.delete.confirmNote"
                    }),
                    name.clone(),
                ),
                // 지금 앱: <b>이름</b> 다이어그램을 삭제할까요? + 휴지통 안내 한 줄
                day_card::Confirm::Diagram { name, is_dir, .. } => (
                    t(if *is_dir {
                        "diagrams.delete.folderTitle"
                    } else {
                        "diagrams.delete.fileTitle"
                    }),
                    format!(
                        "{{name}} {}\n{}",
                        t(if *is_dir {
                            "diagrams.delete.bodyDir"
                        } else {
                            "diagrams.delete.bodyFile"
                        }),
                        t("diagrams.delete.trashNote")
                    ),
                    name.clone(),
                ),
                day_card::Confirm::Comment { name, .. } => (
                    t("notes.cmt.deleteTitle"),
                    i18n::t_with("notes.cmt.deleteConfirm", &[("name", "{name}")]),
                    name.clone(),
                ),
                day_card::Confirm::Block { name, .. } => (
                    t("todos.tt.deleteBlockTitle"),
                    i18n::t_with("todos.tt.deleteBlockConfirm", &[("name", "{name}")]),
                    name.clone(),
                ),
            };
            dialog::confirm_delete(
                &k,
                title,
                body,
                name,
                cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                }),
                cx.listener(|this, _, _, cx| {
                    this.confirmed(cx);
                    cx.notify();
                }),
            )
        });

        div()
            .relative()
            .size_full()
            .flex()
            .bg(k.surface)
            .text_color(fg)
            .track_focus(&self.focus)
            // 포커스가 이 화면 밖으로 빠져 있으면(입력칸이 닫혀 사라진 뒤 등) 누를 때 되찾는다
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.focus.contains_focused(window, cx) {
                        this.focus.focus(window, cx);
                    }
                }),
            )
            // ⌥⌘C — 노트 탭에서 지금 노트의 경로 복사(NoteCrumb)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                let m = &e.keystroke.modifiers;
                // 확대 뷰어가 맨 위 — Esc 는 그것부터 닫는다
                if this.mmd_zoom.is_some() && e.keystroke.key == "escape" {
                    this.mmd_zoom = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                // 다이어그램 캔버스 — + - 1 0 /(손이 올라가 있을 때), Esc(전체 화면, 선택), ⌘F(찾기).
                // 원문 칸이나 찾기 칸에서 치는 글자는 건드리지 않는다
                if this.section == Section::Diagrams && !this.diagram_typing(window, cx) {
                    let key = e.keystroke.key.as_str();
                    let find = (m.platform && !m.alt && !m.shift && key == "f")
                        || (!m.platform
                            && !m.control
                            && key == "/"
                            && this.diagrams.canvas.pointer_in(window));
                    if find {
                        this.open_diagram_search(m.platform, window, cx);
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if !m.platform && !m.control && this.canvas_key(key, window) {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                }
                // ERD — Esc 는 안쪽부터([보기] 판, 정보창, 그다음 패널), ⌘F 는 그린 표와 컬럼에서 찾기
                if this.section == Section::Erd
                    && !this.erd.overview
                    && e.keystroke.key == "escape"
                    && this.erd_escape(cx)
                {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.section == Section::Erd
                    && !this.erd.overview
                    && m.platform
                    && !m.alt
                    && !m.shift
                    && e.keystroke.key == "f"
                {
                    this.erd_open_find(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.section == Section::Notes
                    && m.platform
                    && m.alt
                    && !m.control
                    && !m.shift
                    && e.keystroke.key == "c"
                {
                    this.copy_note_path(cx);
                }
                // ⌘F — 노트 안에서 찾기(트리 판에 포커스가 있으면 트리 찾기가 먼저 받아 멈춘다)
                if this.section == Section::Notes
                    && m.platform
                    && !m.alt
                    && !m.shift
                    && e.keystroke.key == "f"
                {
                    this.start_page_find(window, cx);
                    cx.notify();
                }
                // ⌘S — 다이어그램을 고치는 중일 때 저장
                if this.section == Section::Diagrams
                    && this.diagrams.edit.is_some()
                    && m.platform
                    && !m.alt
                    && e.keystroke.key == "s"
                {
                    this.save_diagram(false, cx);
                    cx.notify();
                }
                // ⌘S — 노트를 고치는 중일 때 저장
                if this.section == Section::Notes
                    && this.notes.edit.is_some()
                    && m.platform
                    && !m.alt
                    && e.keystroke.key == "s"
                {
                    this.save_note(false, cx);
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, _: &row_drag::DragRow, _, cx| {
                this.drop_row();
                cx.notify();
            }))
            .on_drop(cx.listener(|this, _: &tt_drag::TtGrab, window, cx| {
                this.tt_drop(window, cx);
                cx.notify();
            }))
            // 다이어그램 캔버스 끌어 옮기기 — 판 밖으로 나가도 따라간다
            .on_drag_move::<diagram_canvas::CanvasGrab>(cx.listener(
                |this, e: &DragMoveEvent<diagram_canvas::CanvasGrab>, _, cx| {
                    if this.canvas_pan_to(e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(|this, _: &diagram_canvas::CanvasGrab, _, cx| {
                this.canvas_drop();
                cx.notify();
            }))
            .on_drag_move::<erd_view::ErdGrab>(cx.listener(
                |this, e: &DragMoveEvent<erd_view::ErdGrab>, _, cx| {
                    if this.erd_pan_to(e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(|this, _: &erd_view::ErdGrab, _, cx| {
                this.erd_drop();
                cx.notify();
            }))
            .on_drag_move::<erd_view::ErdPanelGrab>(cx.listener(
                |this, e: &DragMoveEvent<erd_view::ErdPanelGrab>, _, cx| {
                    if this.erd_panel_to(e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(|this, _: &erd_view::ErdPanelGrab, _, cx| {
                this.erd_panel_drop();
                cx.notify();
            }))
            .on_drag_move::<mermaid_zoom::ZoomGrab>(cx.listener(
                |this, e: &DragMoveEvent<mermaid_zoom::ZoomGrab>, _, cx| {
                    if this.zoom_pan_to(e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(|this, _: &mermaid_zoom::ZoomGrab, _, cx| {
                this.zoom_drop();
                cx.notify();
            }))
            .on_drag_move::<pane::PaneGrab>(cx.listener(
                |this, e: &DragMoveEvent<pane::PaneGrab>, _, cx| {
                    if this.pane_move(e.event.position.x) {
                        cx.notify();
                    }
                },
            ))
            .on_drop(cx.listener(|this, g: &tree_drag::TreeGrab, _, cx| {
                this.tree_drop(g.which);
                cx.notify();
            }))
            .on_drop(cx.listener(|this, _: &pane::PaneGrab, _, cx| {
                this.pane_drop();
                cx.notify();
            }))
            .child(self.rail(&k, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(k.surface)
                    .child(
                        // .topbar: 위 10px, 옆 8+6px, 아래 2px. 제목 .brand 15px 700
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .pt(px(10.))
                            .px(px(14.))
                            .pb(px(2.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(t(RAIL.iter().find(|r| r.0 == self.section).unwrap().2)),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(body)),
            )
            .children(self.drop_line(&k))
            .children(self.tree_drag_ghost(&k))
            .children(self.comment_fab(&k, cx))
            .children(ai_modal)
            .children(dgm_full)
            .children(dgm_prompt)
            .children(dgm_ai)
            .children(span_modal)
            .children(match self.section {
                Section::Diagrams => {
                    self.name_modal_view(vault_pane::Which::Diagrams, &k, window, cx)
                }
                _ => self.name_modal_view(vault_pane::Which::Notes, &k, window, cx),
            })
            .children(self.note_prompt(&k, cx))
            .children(confirm)
            .children(self.mmd_zoom_view(&k, cx))
    }
}

impl Spike {
    /// 본문 링크 — `#2-3` 은 이 노트의 절로, 나머지는 브라우저로
    pub(crate) fn on_link(&self, cx: &mut Context<Self>) -> markdown::OnLink {
        let this = cx.entity().downgrade();
        Arc::new(move |url, _, _, cx| match url.strip_prefix('#') {
            Some(sec) => {
                let _ = this.update(cx, |s, cx| {
                    s.jump(sec);
                    cx.notify();
                });
            }
            None => cx.open_url(url),
        })
    }

    /// .rail — 68px, 표식 + 작업공간 탭 + (아래로 밀린) 설정. 선을 긋지 않는다: 판과 같은 바닥이다
    fn rail(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        if self.mark.as_ref().is_none_or(|(dark, _)| *dark != k.dark) {
            let src = include_str!("../assets/amber-mark.svg");
            self.mark = figure::svg(src, &hex(k.text_2)).ok().map(|f| (k.dark, f));
        }
        // 백그라운드 작업 점 — 도는 중 = 맥동(무채색), 끝났으니 와서 보라 = 초록 점(움직이지 않는다)
        let notes_busy = self.notes.ai.any_running();
        let notes_done = self.notes.rail_done;
        let reduce = cx.reduce_motion();
        let rail_dot = move |section: Section, k: &Tokens| -> Option<AnyElement> {
            if section != Section::Notes {
                return None;
            }
            let dot = div()
                .absolute()
                .top(px(7.))
                .right(px(11.))
                .size(px(6.))
                .rounded_full();
            if notes_busy {
                Some(if reduce {
                    dot.bg(k.text).opacity(0.85).into_any_element()
                } else {
                    dot.bg(k.text)
                        .with_animation(
                            "rail-busy",
                            Animation::new(std::time::Duration::from_millis(1100)).repeat(),
                            |d, x| {
                                d.opacity(
                                    0.3 + 0.7 * (1. - (std::f32::consts::PI * 2. * x).cos()) / 2.,
                                )
                            },
                        )
                        .into_any_element()
                })
            } else if notes_done {
                Some(
                    dot.bg(k.success)
                        .shadow(vec![BoxShadow {
                            color: k.success.opacity(0.22),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(2.),
                            inset: false,
                        }])
                        .tag("rail-done")
                        .into_any_element(),
                )
            } else {
                None
            }
        };
        let item = |id: &'static str, label: String, icon_name: &str, active: bool| {
            let (fill, edge) = if active {
                (k.glass_fill_accent, k.glass_edge_accent)
            } else {
                (k.glass_fill, k.glass_edge)
            };
            let glass = linear_gradient(
                150.,
                linear_color_stop(fill.0, 0.),
                linear_color_stop(fill.1, 1.),
            );
            div()
                .id(id)
                .tag(id)
                .relative()
                .w(px(54.))
                .pt(px(9.))
                .pb(px(7.))
                .border_1()
                .border_color(transparent_black())
                .rounded(px(12.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(5.))
                .text_color(k.text_3)
                .text_size(px(10.5))
                // 버튼 글자는 줄 높이가 'normal' — 지금 앱 실측 11px(칸 높이 53px)
                .line_height(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(active, |d| {
                    d.bg(glass)
                        .border_color(edge)
                        .shadow(k.glass_shadow(true))
                        .text_color(k.text)
                })
                .when(!active, |d| {
                    let (k2, shadow) = (k.text_2, k.glass_shadow(false));
                    d.hover(move |s| s.bg(glass).border_color(edge).shadow(shadow).text_color(k2))
                })
                .child(icon(icon_name, 20.).text_color(if active { k.text } else { k.text_3 }))
                .child(label)
        };
        div()
            .w(px(68.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .pt(px(14.))
            .pb(px(12.))
            .bg(k.surface)
            .when_some(self.mark.as_ref(), |d, (_, m)| {
                d.child(div().mb(px(16.)).child(img(m.image.clone()).size(px(40.))))
            })
            .children(RAIL.iter().map(|&(section, id, key, icon_name)| {
                item(key, t(key), icon_name, self.section == section)
                    .children(rail_dot(section, k))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        amber_gpui::prefs::set(&this.db, "til.section", id);
                        // 탭에 들어올 때마다 트리를 다시 읽는다(Finder, git, 다른 편집기가 바꾼 것)
                        if section == Section::Notes {
                            this.reload_notes();
                        }
                        if section == Section::Diagrams {
                            this.reload_tree(vault_pane::Which::Diagrams);
                        }
                        cx.notify();
                    }))
            }))
            .child(div().flex_1())
            .child(item(
                "app.rail.settings",
                t("app.rail.settings"),
                "settings",
                false,
            ))
    }
}

/// 토큰 색을 SVG 글자색("#rrggbb")으로
pub(crate) fn hex(c: Hsla) -> String {
    let c = Rgba::from(c);
    let b = |v: f32| (v * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

/// 할 일 탭 달력 판(지금 앱의 CAL_W_MIN/MAX, 키 amber.todo.cal-width)
const CAL_PANE: pane::PaneGrab = pane::PaneGrab {
    key: "amber.todo.cal-width",
    min: 260.,
    max: 780.,
};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    #[cfg(feature = "shots")]
    if let Some(dir) = arg("--shots") {
        return shots::run(&dir);
    }
    let started = Instant::now();
    let db_path = arg("--db").expect("--db <amber.db 사본>");
    let note = arg("--note");
    let date = arg("--date").unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let jump = arg("--jump");

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let bounds = Bounds::centered(None, size(px(1440.), px(920.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Amber GPUI 시험판".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, move |window, cx| {
                let db = Db::open(&db_path, Clock::System).expect("DB 사본을 못 열었다");
                amber_gpui::migrate::run(&db).expect("마이그레이션");
                // 지금 앱에서 고른 언어, 테마, 작업 폴더를 이어받는다(처음 한 번, 아직 없는 키만)
                amber_gpui::prefs::import_webview(&db);
                i18n::init(amber_gpui::prefs::get(&db, "amber.lang").as_deref());
                let pref = amber_gpui::prefs::get(&db, "til.theme");
                theme::apply(pref.as_deref(), window.appearance(), cx);
                cx.new(|cx| Spike::new(db, date, note, jump, started, window, cx))
            })
            .expect("창을 못 열었다");
            cx.activate(true);
        });
}
