// 노트 탭 — 지금 앱 NotesView.tsx. 왼쪽 카드 = 보관함 트리(vault/notes 또는 고른 작업 폴더), 오른쪽 카드 = 고른 노트.
// 트리 행은 세 단계가 서로 다르다: hover = 틴트, 열린 폴더 = 면 유리 + 왼쪽 회색 바, 고른 파일 = 강조 유리 + 왼쪽 글자색 바.
// 펼친 폴더와 고른 노트는 저장하지 않는다(지금 앱도 앱을 다시 열면 처음부터). 탭에 들어올 때마다 트리를 다시 읽는다.

use std::collections::{HashMap, HashSet};

use amber_gpui::i18n::t;
use amber_gpui::vault::{self, Node};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::day_card::{Confirm, Tag, button, danger_button};
use crate::pane::PaneGrab;
use crate::theme::Tokens;
use crate::{Spike, figure, hex, markdown, note};

/// 지금 앱 usePaneResize 의 기본값(PANE_MIN/MAX/DEFAULT), 키 amber.notes.list-width
pub const LIST_PANE: PaneGrab = PaneGrab {
    key: "amber.notes.list-width",
    min: 240.,
    max: 640.,
};
pub const LIST_DEFAULT: f32 = 340.;
/// 본문 그림의 최대 폭 — .md-svg 처럼 본문 폭(720 - 안쪽 여백)을 넘지 않는다
const FIGURE_MAX: f32 = 688.0;

pub enum Block {
    Text(SharedString),
    /// light = 흰 판(mermaid). 지금 앱도 mermaid 는 밝은 판, svg 는 어두운 판에 놓는다
    Figure {
        fig: figure::Figure,
        light: bool,
    },
    Broken(SharedString),
}

#[derive(Default)]
pub struct NotesState {
    /// None = 아직 안 읽음
    pub tree: Option<Vec<Node>>,
    pub error: Option<String>,
    pub expanded: HashSet<String>,
    /// 새 노트/폴더가 들어갈 폴더 — 마지막으로 누른 폴더(또는 연 노트의 폴더)
    pub active_dir: String,
    pub selected: Option<String>,
    pub body: String,
    pub read_error: Option<String>,
    pub blocks: Vec<Block>,
    /// 절 번호 → 본문 판의 자식 번호(제목으로 시작하는 글 덩어리)
    pub sections: HashMap<String, usize>,
    pub width: f32,
    pub copied: bool,
}

/// 노트 본문 → 글 덩어리와 그림. svg 의 currentColor 는 본문 글자색이다
pub fn build_blocks(src: &str, text_hex: &str) -> (Vec<Block>, HashMap<String, usize>) {
    let segments = note::split(src);
    let mut sections = HashMap::new();
    for (i, s) in segments.iter().enumerate() {
        if let note::Segment::Markdown(md) = s
            && let Some(sec) = note::section_of(md)
        {
            // 같은 번호가 또 나오면 앞의 것(querySelector 와 같다)
            sections.entry(sec.to_string()).or_insert(i);
        }
    }
    let blocks = segments
        .into_iter()
        .map(|s| match s {
            note::Segment::Markdown(md) => Block::Text(md.into()),
            note::Segment::Svg(src) => figure::svg(&src, text_hex).map_or_else(
                |e| Block::Broken(format!("svg: {e}").into()),
                |fig| Block::Figure { fig, light: false },
            ),
            note::Segment::Mermaid(src) => figure::mermaid(&src).map_or_else(
                |e| Block::Broken(format!("mermaid: {e}").into()),
                |fig| Block::Figure { fig, light: true },
            ),
        })
        .collect();
    (blocks, sections)
}

/// 'TIL/이름.md' → '이름'
fn file_name(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".md").unwrap_or(name)
}

/// a/b/c → [a, a/b, a/b/c]
fn ancestors(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(part);
        out.push(acc.clone());
    }
    out
}

impl Spike {
    /// 탭에 들어올 때마다 — Finder, git, 다른 편집기가 바꾼 것을 반영한다
    pub(crate) fn reload_notes(&mut self) {
        match vault::notes(&self.db).list_tree() {
            Ok(tree) => {
                self.notes.tree = Some(tree);
                self.notes.error = None;
            }
            Err(e) => self.notes.error = Some(e),
        }
    }

    /// 보관함 이름 — 기본 보관함은 언어를 따르고, 고른 작업 폴더는 폴더 이름 그대로
    fn notes_root_name(&self) -> String {
        match amber_gpui::prefs::get(&self.db, "amber.root.notes").filter(|r| !r.is_empty()) {
            Some(r) => r
                .split('/')
                .filter(|s| !s.is_empty())
                .next_back()
                .unwrap_or(&r)
                .to_string(),
            None => t("settings.root.default"),
        }
    }

    pub(crate) fn open_note(&mut self, rel: &str, cx: &mut Context<Self>) {
        let n = &mut self.notes;
        n.selected = Some(rel.to_string());
        n.copied = false;
        let dir = vault::parent_of(rel).to_string();
        n.expanded.extend(ancestors(&dir));
        n.active_dir = dir;
        match vault::notes(&self.db).read_file(rel) {
            Ok(body) => {
                let text = hex(Tokens::get(cx).text);
                let (blocks, sections) = build_blocks(&body, &text);
                let n = &mut self.notes;
                n.body = body;
                n.blocks = blocks;
                n.sections = sections;
                n.read_error = None;
            }
            Err(e) => {
                let n = &mut self.notes;
                n.body.clear();
                n.blocks.clear();
                n.sections.clear();
                n.read_error = Some(e);
            }
        }
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    fn toggle_dir(&mut self, path: &str) {
        let n = &mut self.notes;
        if !n.expanded.remove(path) {
            n.expanded.insert(path.to_string());
        }
        n.active_dir = path.to_string();
    }

    /// 지우기 확인을 지난 뒤 — 고른 노트가 그 안에 있었으면 닫는다
    pub(crate) fn delete_note(&mut self, rel: &str) -> Result<(), String> {
        let r = vault::delete_note_entry(&self.db, rel);
        let n = &mut self.notes;
        if n.selected
            .as_deref()
            .is_some_and(|s| s == rel || s.starts_with(&format!("{rel}/")))
        {
            n.selected = None;
            n.blocks.clear();
            n.body.clear();
        }
        self.reload_notes();
        r
    }

    /// 절 링크 → 그 번호를 단 제목이 판 맨 위에 오게 (NotesView 의 data-sec 탐색)
    pub(crate) fn jump(&self, sec: &str) {
        if let Some(&ix) = self.notes.sections.get(sec) {
            // 판의 자식은 경로 줄, 제목, 동작 줄 다음에 본문 덩어리
            self.scroll.scroll_to_top_of_item(ix + DETAIL_HEAD_ITEMS);
        }
    }

    pub(crate) fn notes_body(&self, body_w: f32, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let width = crate::pane::effective(self.notes.width, LIST_PANE.min, body_w);
        let card = |d: Div| {
            d.border_1()
                .border_color(k.glass_edge)
                .rounded(px(12.))
                .bg(linear_gradient(
                    150.,
                    linear_color_stop(k.glass_fill.0, 0.),
                    linear_color_stop(k.glass_fill.1, 1.),
                ))
                .shadow(k.glass_shadow(false))
        };
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .flex_shrink_0()
                    .h_full()
                    .pt(px(8.))
                    .pb(px(8.))
                    .pl(px(8.))
                    .pr(px(4.))
                    .child(
                        card(div().size_full().flex().flex_col().overflow_hidden()).children([
                            self.tree_head(k, cx).into_any_element(),
                            self.tree_list(k, cx).into_any_element(),
                        ]),
                    )
                    .child(self.pane_resizer(LIST_PANE, width, body_w, k, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(px(8.))
                    .pb(px(8.))
                    .pr(px(8.))
                    .pl(px(4.))
                    .child(
                        card(div().size_full().overflow_hidden()).child(self.note_detail(k, cx)),
                    ),
            )
    }

    /// .notes-tree-head — 보관함 고르기 | 새 노트, 새 폴더, 찾기, 다시 읽기
    fn tree_head(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let icon_btn = |id: &'static str, name: &str, size: f32| {
            let (bg, text) = (k.surface_2, k.text);
            div()
                .id(id)
                .tag(id)
                .size(px(24.))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .group(id)
                .hover(move |s| s.bg(bg))
                .child(
                    icon(name, size)
                        .text_color(k.text_2)
                        .group_hover(id, move |s| s.text_color(text)),
                )
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.))
            .pt(px(9.))
            .pb(px(9.))
            .pr(px(10.))
            .pl(px(16.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                // .root-picker — 고르기 판은 작업 폴더를 붙일 때(D2)
                div()
                    .id("root-picker")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .max_w(px(170.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text)
                    .cursor_pointer()
                    .hover(|s| s.bg(k.surface_2))
                    .child(icon("folder-open", 14.).text_color(k.text_3))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.notes_root_name()),
                    )
                    .child(icon("chevron-down", 10.).text_color(k.text_3)),
            )
            .child(div().flex_1())
            .child(icon_btn("notes-new-note", "file-plus", 15.))
            .child(icon_btn("notes-new-folder", "folder-plus", 15.))
            .child(icon_btn("notes-find", "search", 14.))
            .child(
                icon_btn("notes-refresh", "refresh", 14.).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.reload_notes();
                        cx.notify();
                    },
                )),
            )
    }

    fn tree_list(&self, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let list = div()
            .id("notes-tree")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(6.))
            .px(px(8.))
            .pb(px(24.));
        if let Some(e) = &self.notes.error {
            return list.child(
                div()
                    .m(px(4.))
                    .text_size(px(12.5))
                    .text_color(k.danger)
                    .child(e.clone()),
            );
        }
        match &self.notes.tree {
            None => list,
            Some(tree) if tree.is_empty() => list.child(
                div()
                    .py(px(44.))
                    .px(px(12.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_size(px(13.))
                    .line_height(relative(1.7))
                    .text_color(k.text_3)
                    .child(t("notes.tree.empty.lead"))
                    .child(t("notes.tree.empty.sub")),
            ),
            Some(tree) => {
                let mut rows = Vec::new();
                self.tree_rows(tree, 0, &mut rows, k, cx);
                list.children(rows)
            }
        }
    }

    /// 펼친 폴더만 자식을 그린다(1,000개 규모에서 접힌 것까지 그리면 수천 개가 살아 있다)
    fn tree_rows(
        &self,
        nodes: &[Node],
        depth: usize,
        out: &mut Vec<AnyElement>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) {
        for n in nodes {
            let open = n.is_dir && self.notes.expanded.contains(&n.path);
            out.push(self.tree_row(n, depth, open, k, cx).into_any_element());
            if open && let Some(kids) = &n.children {
                self.tree_rows(kids, depth + 1, out, k, cx);
            }
        }
    }

    fn tree_row(
        &self,
        n: &Node,
        depth: usize,
        open: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = !n.is_dir && self.notes.selected.as_deref() == Some(n.path.as_str());
        let active = n.is_dir && self.notes.active_dir == n.path;
        let fill = |pair: (Hsla, Hsla)| {
            linear_gradient(
                150.,
                linear_color_stop(pair.0, 0.),
                linear_color_stop(pair.1, 1.),
            )
        };
        // 왼쪽 바(§3 '너 여기 있다') — inset 2px 0 0
        let bar = |color| BoxShadow {
            color,
            offset: point(px(2.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        };
        let path = n.path.clone();
        let is_dir = n.is_dir;
        let tag = format!("tree-{}", n.path);
        div()
            .id(SharedString::from(tag.clone()))
            .tag(tag)
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(8. + depth as f32 * 14.))
            .pr(px(6.))
            .py(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .text_size(px(13.5))
            .text_color(if is_dir || selected { k.text } else { k.text_2 })
            .when(is_dir || selected, |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(!selected && !active, |d| {
                let bg = fill(k.glass_fill);
                d.hover(move |s| s.bg(bg))
            })
            .when(active, |d| {
                let mut shadow = k.glass_shadow(false);
                shadow.push(bar(k.border_strong));
                d.bg(fill(k.glass_fill))
                    .border_color(k.glass_edge)
                    .shadow(shadow)
            })
            .when(selected, |d| {
                let mut shadow = k.glass_shadow(true)[..1].to_vec();
                shadow.extend(k.glass_shadow(false)[1..].iter().cloned());
                shadow.push(bar(k.text));
                d.bg(fill(k.glass_fill_accent))
                    .border_color(k.glass_edge_accent)
                    .shadow(shadow)
            })
            .child(div().w(px(13.)).flex_shrink_0().when(is_dir, |d| {
                d.child(
                    icon(
                        if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        13.,
                    )
                    .text_color(k.text_3),
                )
            }))
            .child(
                icon(
                    if !is_dir {
                        "file"
                    } else if open {
                        "folder-open"
                    } else {
                        "folder"
                    },
                    14.,
                )
                .flex_shrink_0()
                .text_color(if is_dir || selected {
                    k.text_2
                } else {
                    k.text_3
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(n.name.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if is_dir {
                    this.toggle_dir(&path);
                } else if this.notes.selected.as_deref() != Some(path.as_str()) {
                    this.open_note(&path, cx);
                }
                cx.notify();
            }))
    }

    /// 오른쪽 카드 — 고른 노트(경로 줄, 제목, 동작 줄, 본문) 또는 빈 안내
    fn note_detail(&self, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let scroll = div()
            .id("note")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pt(px(32.))
            .pb(px(32.))
            .px(px(40.))
            .flex()
            .flex_col();
        let Some(rel) = self.notes.selected.clone() else {
            return scroll.child(self.notes_empty(k));
        };
        let name = file_name(&rel).to_string();
        let dirs: Vec<&str> = rel.split('/').collect();
        let crumb = std::iter::once(self.notes_root_name())
            .chain(dirs[..dirs.len() - 1].iter().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(" › ");
        let ok = self.notes.read_error.is_none();
        let copied = self.notes.copied;
        let labeled = |id: &'static str, icon_name: &'static str, label: String, enabled: bool| {
            button(id, k, false, !enabled)
                .child(icon(icon_name, 14.).text_color(if enabled { k.text_2 } else { k.text_3 }))
                .child(label)
        };
        let actions = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .py(px(12.))
            .mb(px(20.))
            .border_t_1()
            .border_b_1()
            .border_color(k.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    // 편집, AI 작성, 절 수정은 다음 단계에서(D2c, D2f)
                    .child(labeled("note-edit", "pencil", t("notes.edit"), false))
                    .child(labeled("note-ai", "sparkles", t("notes.aiWrite"), false))
                    .child(
                        labeled(
                            "note-copy",
                            if copied { "check" } else { "copy" },
                            t(if copied { "notes.copied" } else { "notes.copy" }),
                            ok && !self.notes.body.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                this.notes.body.clone(),
                            ));
                            this.notes.copied = true;
                            cx.notify();
                        })),
                    )
                    .child(labeled(
                        "note-span-ai",
                        "scissors",
                        t("notes.spanAi.secBtn"),
                        false,
                    )),
            )
            .child({
                let (rel, name) = (rel.clone(), name.clone());
                danger_button("note-delete", k)
                    .child(icon("trash", 14.).text_color(k.danger))
                    .child(t("common.delete"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(Confirm::Note {
                            rel: rel.clone(),
                            name: name.clone(),
                            is_dir: false,
                        });
                        cx.notify();
                    }))
            });
        let head = [
            // .note-crumb — 경로 줄(복사는 D2 경로 복사에서)
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .mb(px(4.))
                .text_size(px(12.))
                .text_color(k.text_3)
                .child(icon("folder", 12.).text_color(k.text_3))
                .child(crumb)
                .into_any_element(),
            // .detail-title
            div()
                .mb(px(6.))
                .text_size(px(23.))
                .line_height(relative(1.3))
                .font_weight(FontWeight::BOLD)
                .text_color(k.text)
                .child(name)
                .into_any_element(),
            actions.into_any_element(),
        ];
        let on_link = self.on_link(cx);
        let body = self.notes.blocks.iter().enumerate().map(|(i, b)| match b {
            Block::Text(md) => markdown::text(
                ElementId::NamedInteger("md".into(), i as u64),
                md.clone(),
                on_link.clone(),
            )
            .into_any_element(),
            Block::Figure { fig, light } => div()
                .my(px(8.))
                .p_4()
                .border_1()
                .border_color(k.border)
                .rounded_md()
                .when(*light, |d| d.bg(gpui_kit::white()))
                .flex()
                .justify_center()
                .child({
                    let w = fig.width.min(FIGURE_MAX);
                    img(fig.image.clone())
                        .w(px(w))
                        .h(px(fig.height * w / fig.width))
                })
                .into_any_element(),
            Block::Broken(msg) => div()
                .text_color(k.danger)
                .child(msg.clone())
                .into_any_element(),
        });
        scroll
            .children(head)
            .when_some(self.notes.read_error.clone(), |d, e| {
                d.child(div().text_size(px(13.)).text_color(k.danger).child(
                    amber_gpui::i18n::t_with("notes.readError", &[("error", &e)]),
                ))
            })
            .children(body)
    }

    /// .notes-empty — 아무 노트도 고르지 않았을 때
    fn notes_empty(&self, k: &Tokens) -> Div {
        div()
            .size_full()
            .min_h(px(420.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .child(
                div()
                    .size(px(68.))
                    .mb(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(18.))
                    .bg(k.surface_2)
                    .border_1()
                    .border_color(k.border)
                    .child(icon("book", 30.).text_color(k.text_3)),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .text_color(k.text)
                    .child(t("notes.title")),
            )
            .child(
                div()
                    .mt(px(6.))
                    .mb(px(18.))
                    .max_w(px(380.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_size(px(13.))
                    .line_height(relative(1.7))
                    .text_color(k.text_3)
                    .child(t("notes.empty.lead"))
                    .child(t("notes.empty.sub")),
            )
    }
}

/// 본문 판에서 글 덩어리 앞에 놓인 자식 수(경로 줄, 제목, 동작 줄)
const DETAIL_HEAD_ITEMS: usize = 3;
