// GPUI 시험판: 지금 앱의 셸(왼쪽 레일 + 작업 영역) 위에 할 일 하루치(DB 사본)와 노트 하나(글, svg, mermaid).
// 확인하려는 것 — 한글 입력(IME), 그림이 지금 앱과 같게 나오는지, 시작 시간과 메모리. 탭 내용은 하나씩 옮긴다.
//
//   cargo run --release -- --db <amber.db 사본> --note <노트.md> [--date YYYY-MM-DD] [--jump 2-3]

mod assets;
mod figure;
mod markdown;
mod note;
mod theme;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use amber_gpui::db::{Clock, Db, Row, get_i64};
use amber_gpui::todos;
use gpui_kit::component::Disableable;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::theme::Tokens;
use amber_gpui::i18n::{self, t};

const FIGURE_MAX: f32 = 688.0;

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

struct Line {
    id: i64,
    parent_id: Option<i64>,
    content: SharedString,
    done: bool,
    depth: usize,
    /// 이 날짜에서 다른 날로 옮겨 간 기록(고스트) — 지금 앱처럼 흐리게
    carried: bool,
    gone: bool,
}

enum Block {
    Text(SharedString),
    /// light = 흰 판(mermaid). 지금 앱도 mermaid 는 밝은 판, svg 는 어두운 판에 놓는다
    Figure {
        fig: figure::Figure,
        light: bool,
    },
    Broken(SharedString),
}

struct Spike {
    db: Db,
    date: String,
    lines: Vec<Line>,
    input: Entity<InputState>,
    note_title: SharedString,
    blocks: Vec<Block>,
    /// 절 번호 → 노트 판의 자식 번호(제목으로 시작하는 글 덩어리)
    sections: HashMap<String, usize>,
    scroll: ScrollHandle,
    section: Section,
    /// 앱 표식(책상 스탠드). 스탠드 색이 테마를 따르므로 테마별로 굽는다
    mark: Option<(bool, figure::Figure)>,
    started: Instant,
    first_frame: bool,
}

impl Spike {
    fn new(
        db: Db,
        date: String,
        note_path: &str,
        skip: usize,
        jump: Option<String>,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("할 일 추가 — 한글로 입력하고 Enter")
        });
        // 지금 앱의 기본값(system)처럼 macOS 화면 모드를 따라간다
        cx.observe_window_appearance(window, |this: &mut Spike, window, cx| {
            let pref = amber_gpui::prefs::get(&this.db, "til.theme");
            theme::apply(pref.as_deref(), window.appearance(), cx);
            cx.notify();
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
                    if let Err(e) = todos::create_todo(&this.db, &text, &this.date, None, "day") {
                        eprintln!("createTodo: {e}");
                    }
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                    this.reload();
                    cx.notify();
                }
            },
        )
        .detach();

        let source =
            std::fs::read_to_string(note_path).unwrap_or_else(|e| format!("노트를 못 읽었다: {e}"));
        // 노트 svg 의 currentColor = 본문 글자색(--text)
        let text = hex(Tokens::get(cx).text);
        // --skip: 앞 덩어리를 건너뛴다. 스크롤 입력을 못 보내는 환경에서 뒤쪽 그림을 찍으려고 둔다
        let segments: Vec<_> = note::split(&source).into_iter().skip(skip).collect();
        let mut sections = HashMap::new();
        for (i, s) in segments.iter().enumerate() {
            if let note::Segment::Markdown(md) = s
                && let Some(sec) = note::section_of(md)
            {
                // 판의 첫 자식은 노트 제목이라 +1. 같은 번호가 또 나오면 앞의 것(querySelector 와 같다)
                sections.entry(sec.to_string()).or_insert(i + 1);
            }
        }
        let blocks = segments
            .into_iter()
            .map(|s| match s {
                note::Segment::Markdown(md) => Block::Text(md.into()),
                note::Segment::Svg(src) => figure::svg(&src, &text).map_or_else(
                    |e| Block::Broken(format!("svg: {e}").into()),
                    |fig| Block::Figure { fig, light: false },
                ),
                note::Segment::Mermaid(src) => figure::mermaid(&src).map_or_else(
                    |e| Block::Broken(format!("mermaid: {e}").into()),
                    |fig| Block::Figure { fig, light: true },
                ),
            })
            .collect();
        let note_title = std::path::Path::new(note_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into();

        let section = amber_gpui::prefs::get(&db, "til.section")
            .and_then(|id| RAIL.iter().find(|r| r.1 == id))
            .map_or(Section::Todo, |r| r.0);
        let mut this = Spike {
            db,
            date,
            lines: Vec::new(),
            input,
            note_title,
            blocks,
            sections,
            scroll: ScrollHandle::new(),
            section,
            mark: None,
            started,
            first_frame: false,
        };
        this.reload();
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

    /// 절 링크 → 그 번호를 단 제목이 판 맨 위에 오게 (NotesView 의 data-sec 탐색)
    fn jump(&self, sec: &str) {
        if let Some(&ix) = self.sections.get(sec) {
            self.scroll.scroll_to_top_of_item(ix);
        }
    }

    /// listTodos 의 평평한 줄을 화면 순서(부모 아래 자식)로 편다. 부모가 이 날짜에 없는 자식은 맨 위로 — todoTree.visibleRoots
    fn reload(&mut self) {
        let rows = todos::list_todos(&self.db, &self.date).unwrap_or_else(|e| {
            eprintln!("listTodos: {e}");
            Vec::new()
        });
        let ids: HashSet<i64> = rows.iter().filter_map(|r| get_i64(r, "id")).collect();
        let mut kids: HashMap<Option<i64>, Vec<&Row>> = HashMap::new();
        for r in &rows {
            let parent = get_i64(r, "parent_id").filter(|p| ids.contains(p));
            kids.entry(parent).or_default().push(r);
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        fn walk(
            parent: Option<i64>,
            depth: usize,
            kids: &HashMap<Option<i64>, Vec<&Row>>,
            seen: &mut HashSet<i64>,
            out: &mut Vec<Line>,
        ) {
            for r in kids.get(&parent).map(Vec::as_slice).unwrap_or_default() {
                let Some(id) = get_i64(r, "id") else { continue };
                if !seen.insert(id) {
                    continue;
                }
                out.push(Line {
                    id,
                    parent_id: get_i64(r, "parent_id"),
                    content: r
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string()
                        .into(),
                    done: get_i64(r, "done") == Some(1),
                    depth,
                    carried: get_i64(r, "carried") == Some(1),
                    gone: get_i64(r, "gone") == Some(1),
                });
                walk(Some(id), depth + 1, kids, seen, out);
            }
        }
        walk(None, 0, &kids, &mut seen, &mut out);
        self.lines = out;
    }

    fn toggle(&mut self, id: i64, parent_id: Option<i64>, done: bool) {
        let r = todos::set_subtree_done(&self.db, id, if done { 1 } else { 0 })
            .and_then(|_| todos::recompute_chain_from(&self.db, parent_id));
        if let Err(e) = r {
            eprintln!("toggle: {e}");
        }
        self.reload();
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
        let (fg, muted, border) = (k.text, k.text_3, k.border);
        let this = cx.entity().downgrade();
        let on_link: markdown::OnLink = {
            let this = this.clone();
            Arc::new(move |url, _, _, cx| match url.strip_prefix('#') {
                Some(sec) => {
                    let _ = this.update(cx, |s, cx| {
                        s.jump(sec);
                        cx.notify();
                    });
                }
                None => cx.open_url(url),
            })
        };

        let list = div()
            .id("todos")
            .flex()
            .flex_col()
            .gap_1()
            .overflow_y_scroll()
            .children(self.lines.iter().map(|l| {
                let (id, parent, done) = (l.id, l.parent_id, l.done);
                let this = this.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl(px(l.depth as f32 * 20.0))
                    .child(
                        Checkbox::new(ElementId::NamedInteger("todo".into(), id as u64))
                            .checked(done)
                            .disabled(l.gone)
                            .on_click(move |checked, _, cx| {
                                let checked = *checked;
                                let _ = this.update(cx, |s, cx| {
                                    s.toggle(id, parent, checked);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_color(if l.carried || done { muted } else { fg })
                            .when(done, |d| d.line_through())
                            .child(l.content.clone()),
                    )
                    .when(l.carried, |d| {
                        d.child(div().text_xs().text_color(muted).child(if l.gone {
                            "지워짐"
                        } else {
                            "옮김"
                        }))
                    })
            }));

        let note = div()
            .id("note")
            .flex_1()
            // 없으면 긴 줄이 판 너비를 밀어내 접히지 않는다
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_6()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .text_color(fg)
                    .child(self.note_title.clone()),
            )
            .children(self.blocks.iter().enumerate().map(|(i, b)| {
                match b {
                    Block::Text(md) => markdown::text(
                        ElementId::NamedInteger("md".into(), i as u64),
                        md.clone(),
                        on_link.clone(),
                    )
                    .into_any_element(),
                    Block::Figure { fig, light } => div()
                        .p_4()
                        .border_1()
                        .border_color(border)
                        .rounded_md()
                        .when(*light, |d| d.bg(gpui_kit::white()))
                        .flex()
                        .justify_center()
                        .child({
                            // 지금 앱의 .md-svg 처럼 본문 폭(720 - 안쪽 여백)을 넘지 않게 줄인다
                            let w = fig.width.min(FIGURE_MAX);
                            img(fig.image.clone())
                                .w(px(w))
                                .h(px(fig.height * w / fig.width))
                        })
                        .into_any_element(),
                    Block::Broken(msg) => div()
                        .text_color(gpui_kit::red())
                        .child(msg.clone())
                        .into_any_element(),
                }
            }));

        let body = match self.section {
            Section::Todo => div()
                .size_full()
                .p_5()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_color(muted).child(self.date.clone()))
                .child(Input::new(&self.input))
                .child(list)
                .into_any_element(),
            Section::Notes => note.into_any_element(),
            // 아직 옮기지 않은 탭 — 체크리스트(D)에서 하나씩
            _ => div().into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .bg(k.surface)
            .text_color(fg)
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
    }
}

impl Spike {
    /// .rail — 68px, 표식 + 작업공간 탭 + (아래로 밀린) 설정. 선을 긋지 않는다: 판과 같은 바닥이다
    fn rail(&mut self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        if self.mark.as_ref().is_none_or(|(dark, _)| *dark != k.dark) {
            let src = include_str!("../assets/amber-mark.svg");
            self.mark = figure::svg(src, &hex(k.text_2)).ok().map(|f| (k.dark, f));
        }
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
                item(key, t(key), icon_name, self.section == section).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.section = section;
                        amber_gpui::prefs::set(&this.db, "til.section", id);
                        cx.notify();
                    },
                ))
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
fn hex(c: Hsla) -> String {
    let c = Rgba::from(c);
    let b = |v: f32| (v * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let started = Instant::now();
    let db_path = arg("--db").expect("--db <amber.db 사본>");
    let note_path = arg("--note").expect("--note <노트.md>");
    let date = arg("--date").unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let skip = arg("--skip").and_then(|s| s.parse().ok()).unwrap_or(0);
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
                cx.new(|cx| Spike::new(db, date, &note_path, skip, jump, started, window, cx))
            })
            .expect("창을 못 열었다");
            cx.activate(true);
        });
}
