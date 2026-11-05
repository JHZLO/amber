// 노트 탭 — 지금 앱 NotesView.tsx. 왼쪽 카드 = 보관함 트리(vault/notes 또는 고른 작업 폴더), 오른쪽 카드 = 고른 노트.
// 트리 행은 세 단계가 서로 다르다: hover = 틴트, 열린 폴더 = 면 유리 + 왼쪽 회색 바, 고른 파일 = 강조 유리 + 왼쪽 글자색 바.
// 펼친 폴더와 고른 노트는 저장하지 않는다(지금 앱도 앱을 다시 열면 처음부터). 탭에 들어올 때마다 트리를 다시 읽는다.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use amber_gpui::i18n::{t, t_with};
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
    /// 글 덩어리 — 상태를 직접 들어야 노트 안 찾기가 그려진 글자에서 찾고 칠하고 그 자리로 굴린다
    Text(Entity<gpui_kit::base::text::TextViewState>),
    /// light = 흰 판(mermaid). 지금 앱도 mermaid 는 밝은 판, svg 는 어두운 판에 놓는다
    Figure {
        fig: figure::Figure,
        light: bool,
    },
    Broken(SharedString),
    /// `<details>` 카드 — key 는 판 안의 자리("3", 안쪽은 "3/1"), meta 는 '문단 1, 코드 1'
    Toggle {
        key: String,
        summary: Option<String>,
        meta: String,
        open: bool,
        blocks: Vec<Block>,
    },
}

/// 토글 머리에 흐리게 적는 '안에 든 것' — 이 순서로(mdDetails.ts 의 BLOCK_KINDS)
fn toggle_meta(body: &str) -> String {
    const KINDS: [&str; 6] = ["paragraph", "list", "code", "figure", "table", "toggle"];
    let mut n = [0usize; 6];
    for seg in note::split(body) {
        match seg {
            note::Segment::Markdown(md) => {
                let Ok(tree) = ::markdown::to_mdast(&md, &::markdown::ParseOptions::gfm()) else {
                    continue;
                };
                for c in tree.children().into_iter().flatten() {
                    let k = match c {
                        ::markdown::mdast::Node::Paragraph(_) => 0,
                        ::markdown::mdast::Node::List(_) => 1,
                        ::markdown::mdast::Node::Code(_) => 2,
                        ::markdown::mdast::Node::Table(_) => 4,
                        _ => continue,
                    };
                    n[k] += 1;
                }
            }
            note::Segment::Svg(_) | note::Segment::Mermaid(_) => n[3] += 1,
            note::Segment::Toggle { .. } => n[5] += 1,
        }
    }
    KINDS
        .iter()
        .zip(n)
        .filter(|(_, c)| *c > 0)
        .map(|(k, c)| t_with(&format!("common.toggle.{k}"), &[("n", &c.to_string())]))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 글 덩어리 전부 — 토글 안까지 문서 순서로. 질문 형광펜과 노트 안 찾기가 이 순서의 번호를 쓴다
/// (닫힌 토글 안도 센다 — 지금 앱의 textContent 가 닫힌 details 의 글도 품는 것처럼)
pub(crate) fn text_states(blocks: &[Block]) -> Vec<Entity<gpui_kit::base::text::TextViewState>> {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Text(s) => out.push(s.clone()),
            Block::Toggle { blocks, .. } => out.extend(text_states(blocks)),
            _ => {}
        }
    }
    out
}

fn blocks_of(src: &str, text_hex: &str, prefix: &str, cx: &mut App) -> Vec<Block> {
    note::split(src)
        .into_iter()
        .enumerate()
        .map(|(i, s)| match s {
            note::Segment::Markdown(md) => {
                Block::Text(cx.new(|cx| gpui_kit::base::text::TextViewState::markdown(&md, cx)))
            }
            note::Segment::Svg(src) => figure::svg(&src, text_hex).map_or_else(
                |e| Block::Broken(format!("svg: {e}").into()),
                |fig| Block::Figure { fig, light: false },
            ),
            note::Segment::Mermaid(src) => figure::mermaid(&src).map_or_else(
                |e| Block::Broken(format!("mermaid: {e}").into()),
                |fig| Block::Figure { fig, light: true },
            ),
            note::Segment::Toggle {
                summary,
                open,
                body,
            } => {
                let key = format!("{prefix}{i}");
                Block::Toggle {
                    meta: toggle_meta(&body),
                    blocks: blocks_of(&body, text_hex, &format!("{key}/"), cx),
                    key,
                    summary,
                    open,
                }
            }
        })
        .collect()
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
    /// 목차 — (덩어리 번호, 단계 1~3, 글자). 두 개 이상일 때만 보인다
    pub toc: Vec<(usize, u8, String)>,
    pub mtime: Option<i64>,
    /// 고른 노트 폴더의 실제 자리 — 본문 그림(`_assets/…`)을 푸는 기준
    pub asset_base: Option<SharedString>,
    /// 편집 중이면 초안과 미리보기, 그리고 떠 있는 확인 창(저장 안 한 변경, 디스크 충돌)
    pub edit: Option<crate::note_edit::NoteEdit>,
    pub prompt: Option<crate::note_edit::NotePrompt>,
    /// 새 노트, 새 폴더, 이름 바꾸기 모달
    pub names: Option<crate::note_names::NameModal>,
    /// 트리 끌기: 끄는 중인 것, 행마다(경로 → 자리, 폴더인가)와 트리 판의 자리, 누른 자리
    pub tree_drag: Option<crate::tree_drag::TreeDrag>,
    pub tree_rows: crate::tree_drag::RowBounds,
    pub tree_area: crate::tree_drag::ListBounds,
    pub tree_press: Option<Point<Pixels>>,
    /// 트리에서 찾기(⌘F)
    pub find: Option<crate::note_find::TreeFindState>,
    /// 노트 안에서 찾기(⌘F, 트리 밖에서)
    pub page_find: Option<crate::note_page_find::PageFindState>,
    /// 보관함 감시(FSEvents)와 그 세대 — 뿌리가 바뀌면 옛 감시의 일은 버린다
    pub watcher: Option<notify::RecommendedWatcher>,
    pub watch_epoch: u64,
    /// 감시를 건(또는 걸려다 못 건) 뿌리 — 달라지면 다시 건다
    pub watch_root: Option<String>,
    /// 저장 안 된 편집 중에 바깥이 고친 내용(배너), '내 것 유지'로 넘긴 내용, 다음 그림에서 넣을 초안
    pub disk_change: Option<crate::note_watch::DiskChange>,
    pub dismissed_disk: Option<String>,
    pub pending_draft: Option<String>,
    /// 작업 폴더 고르기 판이 열렸는가, 트리거 자리
    pub root_menu: bool,
    pub root_anchor: Rc<Cell<Bounds<Pixels>>>,
    pub width: f32,
    pub copied: bool,
    /// 경로 줄을 눌러 복사한 직후(1.5초 동안 체크 표식)
    pub crumb_copied: bool,
    /// 기본(open 속성)에서 뒤집힌 토글 — 노트를 바꾸면 비운다
    pub toggled: HashSet<String>,
    /// 그린 뒤 잰 자리 — 덩어리마다(창 좌표), 머리(경로 줄 ~ 동작 줄) 높이
    pub marks: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    pub head_h: Rc<Cell<Pixels>>,
    /// 인라인 질문(사이드카) — note_comments.rs
    pub cmts: crate::note_comments::Cmts,
    /// AI 전문 작성/부분 수정 실행 — note_ai.rs. 노트를 옮겨 다녀도 남는다
    pub ai: crate::note_ai::Runs,
    /// 부분 수정의 구간 사이에서 멈추라는 깃발(취소 키 → 깃발)
    pub ai_stops: HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// 레일 완료 점 — 다른 탭에 있을 때 작성이 끝났다. 노트 탭에 들어가면 꺼진다
    pub rail_done: bool,
    /// AI 작성 모달 — note_ai_view.rs. 닫기는 숨기기(실행은 스토어에서 계속)
    pub ai_modal: Option<crate::note_ai_view::AiModal>,
    /// 적용한 AI 결과 — render 가 편집 초안으로 넣는다(입력칸은 창이 있어야 바뀐다)
    pub pending_apply: Option<String>,
}

/// 본문 판의 위 여백(.detail padding-top)
const DETAIL_PAD_T: f32 = 32.;
/// 목차가 보이는 창 폭 — 더 좁으면 숨기고 본문 폭 제한도 푼다(@media max-width: 1140px)
const TOC_MIN_WINDOW: f32 = 1140.;

/// 지금 앱 timeAgo — 30일 넘으면 날짜
pub(crate) fn time_ago(ms: i64, now: i64) -> String {
    use amber_gpui::i18n::t_with;
    let m = (now - ms).div_euclid(60_000);
    if m < 1 {
        return t("common.timeago.now");
    }
    if m < 60 {
        return t_with("common.timeago.minutes", &[("m", &m.to_string())]);
    }
    let h = m / 60;
    if h < 24 {
        return t_with("common.timeago.hours", &[("h", &h.to_string())]);
    }
    let d = h / 24;
    if d < 30 {
        return t_with("common.timeago.days", &[("d", &d.to_string())]);
    }
    let date = chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.with_timezone(&chrono::Local).date_naive())
        .unwrap_or_default();
    // toLocaleDateString — ko "2026. 11. 5.", en "11/5/2026"
    use chrono::Datelike;
    if amber_gpui::i18n::is_ko() {
        format!("{}. {}. {}.", date.year(), date.month(), date.day())
    } else {
        format!("{}/{}/{}", date.month(), date.day(), date.year())
    }
}

/// 노트 본문 → 글 덩어리와 그림. svg 의 currentColor 는 본문 글자색이다
pub fn build_blocks(
    src: &str,
    text_hex: &str,
    cx: &mut App,
) -> (Vec<Block>, HashMap<String, usize>) {
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
    (blocks_of(src, text_hex, "", cx), sections)
}

/// 'TIL/이름.md' → '이름'
pub(crate) fn file_name(rel: &str) -> &str {
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
        let same = n.selected.as_deref() == Some(rel);
        n.selected = Some(rel.to_string());
        n.copied = false;
        let dir = vault::parent_of(rel).to_string();
        n.expanded.extend(ancestors(&dir));
        n.asset_base = Some(
            vault::notes(&self.db)
                .abs(&dir)
                .to_string_lossy()
                .into_owned()
                .into(),
        );
        n.active_dir = dir;
        match vault::notes(&self.db).read_file(rel) {
            Ok(body) => {
                let text = hex(Tokens::get(cx).text);
                let (blocks, sections) = build_blocks(&body, &text, cx);
                let toc = note::split(&body)
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| match s {
                        note::Segment::Markdown(md) => {
                            note::heading_of(md).map(|(lv, text)| (i, lv, text))
                        }
                        _ => None,
                    })
                    .collect();
                let mtime = vault::notes(&self.db).mtime(rel);
                let n = &mut self.notes;
                n.toc = toc;
                n.mtime = mtime;
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
                n.toc.clear();
                n.mtime = None;
                n.read_error = Some(e);
            }
        }
        self.notes.marks.borrow_mut().clear();
        self.notes.toggled.clear();
        self.notes.page_find = None;
        self.notes.disk_change = None;
        self.notes.dismissed_disk = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.load_note_comments(rel, same, cx);
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
        if r.is_ok() {
            // 그 아래에서 쓰던 AI 는 끊는다
            self.drop_note_ai_under(rel);
        }
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

    /// 절 링크 → 그 번호를 단 제목으로 (NotesView 의 data-sec 탐색)
    pub(crate) fn jump(&self, sec: &str) {
        if let Some(&ix) = self.notes.sections.get(sec) {
            self.scroll_to_block(ix);
        }
    }

    /// 노트의 실제 경로를 클립보드로 — 경로 줄 클릭과 ⌥⌘C
    pub(crate) fn copy_note_path(&mut self, cx: &mut Context<Self>) {
        let Some(rel) = self.notes.selected.clone() else {
            return;
        };
        let abs = vault::notes(&self.db).abs(&rel);
        cx.write_to_clipboard(ClipboardItem::new_string(
            abs.to_string_lossy().into_owned(),
        ));
        self.notes.crumb_copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |s, cx| {
                s.notes.crumb_copied = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// 그 덩어리가 판 맨 위에서 18px 아래 오게(scroll-margin-top 18px)
    fn scroll_to_block(&self, ix: usize) {
        let Some(b) = self.notes.marks.borrow().get(&ix).copied() else {
            return;
        };
        let view = self.scroll.bounds();
        let off = self.scroll.offset();
        let y = (off.y - (b.origin.y - view.origin.y) + px(18.)).min(px(0.));
        self.scroll.set_offset(point(off.x, y));
    }

    /// 스크롤 스파이 — 판 위에서 96px 안에 들어온 마지막 제목. 처음엔 첫 제목
    fn active_heading(&self) -> Option<usize> {
        let marks = self.notes.marks.borrow();
        let top = self.scroll.bounds().origin.y;
        let mut cur = self.notes.toc.first().map(|t| t.0)?;
        for (ix, ..) in &self.notes.toc {
            match marks.get(ix) {
                Some(b) if b.origin.y - top <= px(96.) => cur = *ix,
                Some(_) => break,
                None => {}
            }
        }
        Some(cur)
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
                    .child({
                        // 찾기 줄의 몇 개는 트리를 거른 결과로 센다
                        let count = match (self.notes.find.as_ref(), self.notes.tree.as_ref()) {
                            (Some(f), Some(tree)) => {
                                amber_gpui::tree_find::filter_tree(tree, &f.query)
                                    .matches
                                    .len()
                            }
                            _ => 0,
                        };
                        // 트리 판 — 누르면 포커스를 가져가 그 뒤의 ⌘F 가 트리 찾기로 온다(본문의 ⌘F 와 안 겹치게)
                        card(
                            div()
                                .track_focus(&self.tree_focus)
                                .size_full()
                                .flex()
                                .flex_col()
                                .overflow_hidden()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        if !this.tree_focus.contains_focused(window, cx) {
                                            this.tree_focus.focus(window, cx);
                                        }
                                    }),
                                )
                                // 바깥을 누르면 트리에서 포커스를 놓는다 — 그래야 본문에서의 ⌘F 가 노트 안 찾기로 간다
                                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                                    if this.tree_focus.is_focused(window) {
                                        this.focus.focus(window, cx);
                                    }
                                }))
                                .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                                    let m = &e.keystroke.modifiers;
                                    if m.platform && !m.alt && e.keystroke.key == "f" {
                                        this.start_tree_find(window, cx);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                })),
                        )
                        .children([self.tree_head(k, cx).into_any_element()])
                        .children(self.tree_find_bar(count, k, cx))
                        .child(self.tree_list(k, cx))
                    })
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
                        card(div().relative().size_full().overflow_hidden())
                            .child(self.note_detail(body_w + 68. > TOC_MIN_WINDOW, k, cx))
                            .children(self.comment_layer(k, cx))
                            .children(self.page_find_bar(k, cx)),
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
            .child({
                // .root-picker — 누르면 기본 보관함, 최근 폴더, 폴더 열기
                let anchor = self.notes.root_anchor.clone();
                div()
                    .id("root-picker")
                    .tag("root-picker")
                    .relative()
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
                    .child(icon("chevron-down", 10.).text_color(k.text_3))
                    .child(
                        canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notes.root_menu = !this.notes.root_menu;
                        cx.notify();
                    }))
            })
            .children(self.root_menu(k, cx))
            .child(div().flex_1())
            .child(
                icon_btn("notes-new-note", "file-plus", 15.).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.open_name_modal(
                            crate::note_names::NameKind::NewNote,
                            None,
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_btn("notes-new-folder", "folder-plus", 15.).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.open_name_modal(
                            crate::note_names::NameKind::NewFolder,
                            None,
                            window,
                            cx,
                        );
                        cx.notify();
                    },
                )),
            )
            .child(icon_btn("notes-find", "search", 14.).on_click(cx.listener(
                |this, _, window, cx| {
                    this.start_tree_find(window, cx);
                    cx.notify();
                },
            )))
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
        // 이번 그림의 행 자리만 남긴다 — 접혀 사라진 행이 놓을 곳으로 잡히지 않게
        self.notes.tree_rows.borrow_mut().clear();
        let area = self.notes.tree_area.clone();
        let drop_root = self
            .notes
            .tree_drag
            .as_ref()
            .is_some_and(|d| d.target.as_deref() == Some(""));
        let list = div()
            .id("notes-tree")
            .tag("notes-tree")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(6.))
            .px(px(8.))
            .pb(px(24.))
            // 루트(트리 여백)로 놓을 때 — 판 안쪽 2px 링
            .when(drop_root, |d| {
                d.rounded(px(8.)).shadow(vec![Tokens::ring(k.text_3, 2.)])
            })
            .on_drag_move::<crate::tree_drag::TreeGrab>(cx.listener(
                |this, e: &DragMoveEvent<crate::tree_drag::TreeGrab>, _, cx| {
                    if this.tree_drag_move(e.event.position) {
                        cx.notify();
                    }
                },
            ))
            .child(
                canvas(move |b, _, _| area.set(b), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            );
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
                // 찾는 중이면 맞은 것과 그 조상만, 조상은 임시로 펼친다
                let found = self
                    .notes
                    .find
                    .as_ref()
                    .map(|f| amber_gpui::tree_find::filter_tree(tree, &f.query));
                let hit = self.notes.find.as_ref().and_then(|f| {
                    let cur = found.as_ref()?.matches.get(f.idx)?.0.clone();
                    Some((f.query.clone(), cur))
                });
                let nodes = found
                    .as_ref()
                    .map_or(tree.as_slice(), |f| f.nodes.as_slice());
                let auto: Vec<String> =
                    found.as_ref().map(|f| f.expand.clone()).unwrap_or_default();
                if found.is_some() && nodes.is_empty() {
                    return list.child(
                        div()
                            .px(px(10.))
                            .py(px(14.))
                            .text_size(px(12.5))
                            .text_color(k.text_3)
                            .child(t("common.find.treeEmpty")),
                    );
                }
                let mut rows = Vec::new();
                self.tree_rows(nodes, 0, &auto, hit.as_ref(), &mut rows, k, cx);
                list.children(rows)
            }
        }
    }

    /// 펼친 폴더만 자식을 그린다(1,000개 규모에서 접힌 것까지 그리면 수천 개가 살아 있다)
    #[allow(clippy::too_many_arguments)]
    fn tree_rows(
        &self,
        nodes: &[Node],
        depth: usize,
        auto: &[String],
        hit: Option<&(String, String)>,
        out: &mut Vec<AnyElement>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) {
        for n in nodes {
            let open =
                n.is_dir && (self.notes.expanded.contains(&n.path) || auto.contains(&n.path));
            out.push(self.tree_row(n, depth, open, hit, k, cx).into_any_element());
            if open && let Some(kids) = &n.children {
                self.tree_rows(kids, depth + 1, auto, hit, out, k, cx);
            }
        }
    }

    fn tree_row(
        &self,
        n: &Node,
        depth: usize,
        open: bool,
        hit: Option<&(String, String)>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // 찾기의 지금 결과 — 행 배경은 그대로 두고(연 것과 헷갈리지 않게) 가는 테와 노란 글자만
        let find_current = hit.is_some_and(|(_, cur)| *cur == n.path);
        let label = crate::note_find::label(&n.name, hit.map(|(q, _)| q.as_str()), find_current, k);
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
        let actions = self.tree_row_actions(n, k, cx);
        // AI 실행 점 — 쓰는 중 / 준비됨 / 실패(정확한 말은 배너가 갖는다)
        let ai_dot = self.ai_tree_dot(&n.path, k, cx.reduce_motion());
        let drag = self.notes.tree_drag.as_ref();
        let lifted = drag.is_some_and(|d| d.grab.path == n.path);
        let target = is_dir && drag.is_some_and(|d| d.target.as_deref() == Some(n.path.as_str()));
        let rows = self.notes.tree_rows.clone();
        let (rec_path, pad) = (n.path.clone(), px(8. + depth as f32 * 14.));
        let this = cx.entity().downgrade();
        let grab = crate::tree_drag::TreeGrab {
            path: n.path.clone(),
            is_dir,
            name: n.name.clone(),
        };
        div()
            .id(SharedString::from(tag.clone()))
            .tag(tag)
            .group("tree-row")
            .relative()
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
            .when(!selected && !active && drag.is_none(), |d| {
                let bg = fill(k.glass_fill);
                d.hover(move |s| s.bg(bg))
            })
            // 들린 원본은 자리에 흐리게 남는다
            .when(lifted, |d| d.opacity(0.4))
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
            // 놓을 폴더 — 채움 + 굵은 링(받아들이는 표시)
            .when(target, |d| {
                d.bg(k.surface_3)
                    .shadow(vec![Tokens::ring(k.text_3, 2.)])
                    .text_color(k.text)
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
                    } else if open || target {
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
                    .child(label),
            )
            .when(find_current && !selected, |d| {
                d.shadow(vec![Tokens::ring(k.border_strong, 1.)])
            })
            .children(ai_dot)
            .child(actions)
            .child(
                canvas(
                    move |b, _, _| {
                        rows.borrow_mut().insert(rec_path, (b, is_dir));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    this.notes.tree_press = Some(e.position);
                }),
            )
            .on_drag(
                grab,
                move |g: &crate::tree_drag::TreeGrab, _, _, cx: &mut App| {
                    let g = g.clone();
                    let _ = this.update(cx, |s, cx| {
                        s.begin_tree_drag(g, pad);
                        cx.notify();
                    });
                    cx.new(|_| crate::tree_drag::Nothing)
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if is_dir {
                    this.toggle_dir(&path);
                } else if this.notes.selected.as_deref() != Some(path.as_str()) {
                    this.request_open_note(&path, cx);
                }
                cx.notify();
            }))
    }

    /// 오른쪽 카드 — 고른 노트(경로 줄, 제목, 동작 줄, 본문) 또는 빈 안내
    fn note_detail(&self, wide: bool, k: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let scroll = div()
            .id("note")
            .tag("note-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pt(px(DETAIL_PAD_T))
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
        let editing = self.notes.edit.is_some();
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
            .child(if editing {
                self.note_edit_actions(k, cx)
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        labeled("note-edit", "pencil", t("notes.edit"), ok).when(ok, |d| {
                            d.on_click(cx.listener(|this, _, window, cx| {
                                this.start_note_edit(window, cx);
                                cx.notify();
                            }))
                        }),
                    )
                    .child({
                        let on = ok && self.ai_connected();
                        labeled("note-ai", "sparkles", t("notes.aiWrite"), on).when(on, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.open_ai_modal(cx)))
                        })
                    })
                    .child(
                        labeled(
                            "note-copy",
                            if copied { "check" } else { "copy" },
                            t(if copied { "notes.copied" } else { "notes.copy" }),
                            ok && !self.notes.body.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            // 정본은 마크다운 원문 — 편집 중이면 초안
                            let text = this
                                .note_draft(cx)
                                .unwrap_or_else(|| this.notes.body.clone());
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            this.notes.copied = true;
                            cx.notify();
                        })),
                    )
                    .child(labeled(
                        "note-span-ai",
                        "scissors",
                        t("notes.spanAi.secBtn"),
                        false,
                    ))
            })
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
        let head_h = self.notes.head_h.clone();
        let head = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            // .note-crumb — 경로 줄이 곧 경로 복사 버튼이다(⌥⌘C 도 같다). 손이 닿으면 면과 끝의 복사 표식
            .child(div().flex().mb(px(4.)).child({
                let copied = self.notes.crumb_copied;
                let (bg, hover_fg) = (k.surface_2, k.text_2);
                div()
                    .id("note-crumb")
                    .tag("note-crumb")
                    .group("note-crumb")
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .ml(px(-6.))
                    .px(px(6.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .hover(move |s| s.bg(bg).text_color(hover_fg))
                    .child(
                        icon("folder", 12.)
                            .text_color(k.text_3)
                            .group_hover("note-crumb", move |s| s.text_color(hover_fg)),
                    )
                    .child(crumb)
                    .child(
                        div()
                            .flex()
                            .when(!copied, |d| {
                                d.opacity(0.).group_hover("note-crumb", |s| s.opacity(1.))
                            })
                            .child(
                                icon(if copied { "check" } else { "copy" }, 11.)
                                    .text_color(k.text_3)
                                    .group_hover("note-crumb", move |s| s.text_color(hover_fg)),
                            ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.copy_note_path(cx);
                    }))
            }))
            // .detail-title — 제목 = 파일명. 누르면 트리의 이름 바꾸기와 같은 모달(연필은 hover 때만)
            .child(
                div().flex().mb(px(6.)).child(
                    div()
                        .id("note-title")
                        .tag("note-title")
                        .group("note-title")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .text_size(px(23.))
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text)
                        .child(name)
                        .child(
                            icon("pencil", 15.)
                                .text_color(k.text_3)
                                .opacity(0.)
                                .group_hover("note-title", |s| s.opacity(1.)),
                        )
                        .on_click({
                            let rel = rel.clone();
                            cx.listener(move |this, _, window, cx| {
                                this.open_name_modal(
                                    crate::note_names::NameKind::Rename {
                                        path: rel.clone(),
                                        is_dir: false,
                                    },
                                    None,
                                    window,
                                    cx,
                                );
                                cx.notify();
                            })
                        }),
                ),
            )
            .child(actions)
            // 백그라운드 AI — 모달을 닫아도 여기서 진행, 결과, 실패를 보고 되돌아간다
            .children(self.ai_banner(k, cx))
            .children(self.disk_banner(k, cx))
            .child(
                canvas(move |b, _, _| head_h.set(b.size.height), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .relative();
        let on_link = self.on_link(cx);
        // 드래그로 고른 글에 [질문] 버튼 — 놓을 때 선택을 읽는다(글자 선택은 TextView 가 먼저 끝낸다)
        let press = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            if this.notes.cmts.sel.is_some() {
                this.note_mouse_down();
                cx.notify();
            }
        });
        let release = cx.listener(|this, e: &MouseUpEvent, _, cx| {
            this.note_mouse_up(e.position, cx);
            cx.notify();
        });
        let body = self.notes.blocks.iter().enumerate().map(|(i, b)| {
            let el = self.note_block(b, i as u64, &on_link, k, cx);
            // 덩어리마다 자리를 잰다 — 절 이동, 목차, 스크롤 스파이가 쓴다
            let marks = self.notes.marks.clone();
            div()
                .id(("note-block", i))
                .tag(format!("note-block-{i}"))
                .relative()
                .child(el)
                .child(
                    canvas(
                        move |b, _, _| {
                            marks.borrow_mut().insert(i, b);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
        });
        let show_toc = wide && self.notes.toc.len() >= 2;
        let column = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            // 목차가 있으면 본문은 720px 까지(넓은 창에서 줄이 너무 길어지지 않게)
            .when(wide, |d| d.max_w(px(720.)))
            .on_mouse_down(MouseButton::Left, press)
            .on_mouse_up(MouseButton::Left, release)
            .children(body);
        let meta = {
            let mut line = format!("{}/{rel}", self.notes_root_name());
            if let Some(ms) = self.notes.mtime {
                let now = chrono::Local::now().timestamp_millis();
                // 지금 앱과 같은 구분자(U+00B7)
                line.push_str(" \u{b7} ");
                line.push_str(&amber_gpui::i18n::t_with(
                    "notes.meta.modified",
                    &[("time", &time_ago(ms, now))],
                ));
            }
            div()
                .mt(px(28.))
                .pt(px(12.))
                .border_t_1()
                .border_color(k.border)
                .text_size(px(12.))
                .text_color(k.text_3)
                .child(line)
        };
        // 편집 중엔 판이 화면 높이에 고정되고 원문과 미리보기가 각자 굴린다(.notes-detail.editing)
        if editing {
            return div()
                .id("note")
                .tag("note-scroll")
                .size_full()
                .pt(px(DETAIL_PAD_T))
                .pb(px(32.))
                .px(px(40.))
                .flex()
                .flex_col()
                .child(head)
                .child(self.note_edit_split(k, cx))
                .child(meta.mt(px(14.)));
        }
        scroll
            .child(head)
            .when_some(self.notes.read_error.clone(), |d, e| {
                d.child(div().text_size(px(13.)).text_color(k.danger).child(
                    amber_gpui::i18n::t_with("notes.readError", &[("error", &e)]),
                ))
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .child(column)
                    .when(show_toc, |d| d.child(self.note_toc(k, cx))),
            )
            .when(self.notes.read_error.is_none(), |d| d.child(meta))
    }

    /// 본문 덩어리 하나 — 글(TextView), 그림, 깨진 그림, 토글(안쪽은 같은 규칙으로 다시)
    pub(crate) fn note_block(
        &self,
        b: &Block,
        id: u64,
        on_link: &markdown::OnLink,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match b {
            Block::Text(state) => {
                let _ = id;
                let this = cx.entity().downgrade();
                markdown::view(state, on_link.clone(), k, self.notes.asset_base.clone())
                    // 찾기가 그 줄을 보여 달라고 하면 본문 판을 굴린다(판이 TextView 밖의 스크롤이라 직접)
                    .on_reveal(move |b, _, cx| {
                        let _ = this.update(cx, |s, _| s.reveal_in_note(b));
                    })
                    .into_any_element()
            }
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
            Block::Toggle {
                key,
                summary,
                meta,
                open,
                blocks,
            } => self.toggle_card(key, summary.as_deref(), meta, *open, blocks, on_link, k, cx),
        }
    }

    /// 마크다운 토글(.md-toggle) — 앉은 유리 카드 한 장이 머리이자 그릇이다. 안쪽 토글은 테만 남긴다
    #[allow(clippy::too_many_arguments)]
    fn toggle_card(
        &self,
        key: &str,
        summary: Option<&str>,
        meta: &str,
        open_default: bool,
        blocks: &[Block],
        on_link: &markdown::OnLink,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = open_default != self.notes.toggled.contains(key);
        let nested = key.contains('/');
        let group: SharedString = format!("toggle-{key}").into();
        let (edge_hover, shadow_hover, text) = (k.glass_edge_hover, k.glass_shadow_hover(), k.text);
        let key_owned = key.to_string();
        let id_base = key
            .bytes()
            .fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64));
        div()
            .my(px(4.))
            .border_1()
            .border_color(k.glass_edge)
            .rounded(px(12.))
            .when(!nested, |d| {
                d.bg(linear_gradient(
                    150.,
                    linear_color_stop(k.glass_fill.0, 0.),
                    linear_color_stop(k.glass_fill.1, 1.),
                ))
                .shadow(k.glass_shadow(false))
            })
            .child(
                div()
                    .id(SharedString::from(format!("toggle-{key}")))
                    .tag(format!("toggle-{key}"))
                    .group(group.clone())
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .pt(px(9.))
                    .pb(px(9.))
                    .pl(px(12.))
                    .pr(px(14.))
                    .rounded(px(12.))
                    .cursor_pointer()
                    .text_size(px(14.))
                    .line_height(relative(1.7))
                    .child(
                        icon(
                            if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            14.,
                        )
                        .flex_shrink_0()
                        // 꺾쇠를 제목 첫 줄 가운데에 — (1.7em - 14px) / 2
                        .mt(px(4.9))
                        .text_color(k.text_3)
                        .group_hover(group.clone(), move |s| s.text_color(text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text)
                            .child(
                                summary.map_or_else(|| t("common.toggle.untitled"), str::to_string),
                            ),
                    )
                    .when(!meta.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .mt(px(2.9))
                                .text_size(px(12.))
                                .line_height(relative(1.5))
                                .text_color(k.text_3)
                                .child(meta.to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.notes.toggled.remove(&key_owned) {
                            this.notes.toggled.insert(key_owned.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(open, |d| {
                d.child(
                    div()
                        .pl(px(34.))
                        .pr(px(16.))
                        .pb(px(12.))
                        // 머리와 몸통 사이 한 줄 — 꺾쇠 자리부터(왼쪽 -22px)
                        .child(div().h(px(1.)).ml(px(-22.)).mb(px(10.)).bg(k.glass_edge))
                        .children(blocks.iter().enumerate().map(|(i, b)| {
                            self.note_block(b, id_base.wrapping_add(i as u64 + 1), on_link, k, cx)
                        })),
                )
            })
            // 머리에 손이 닿으면 카드 테와 빛줄기가 함께 짙어진다
            .group_hover(group, move |s| {
                s.border_color(edge_hover).shadow(shadow_hover)
            })
            .into_any_element()
    }

    /// 오른쪽 목차(.note-toc) — 본문을 굴려도 판 위 4px 에 머문다(sticky). 지금 절은 굵게
    fn note_toc(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let sy = -f32::from(self.scroll.offset().y);
        let wrap_top = DETAIL_PAD_T + f32::from(self.notes.head_h.get());
        let dy = (sy + 4. - wrap_top).max(0.);
        let active = self.active_heading();
        let view_h = f32::from(self.scroll.bounds().size.height);
        div()
            .relative()
            .top(px(dy))
            .ml_auto()
            .w(px(190.))
            .flex_shrink_0()
            .pl(px(14.))
            .border_l_1()
            .border_color(k.border)
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .mb(px(6.))
                    .pl(px(6.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text_3)
                    .child(t("notes.toc").to_uppercase()),
            )
            .child(
                div()
                    .id("note-toc")
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .max_h(px((view_h - 230. + 64.).max(120.)))
                    .overflow_y_scroll()
                    .children(self.notes.toc.iter().map(|(ix, lv, text)| {
                        let ix = *ix;
                        let on = active == Some(ix);
                        let tag = format!("toc-{ix}");
                        let (hover_bg, hover_fg) = (k.surface_2, k.text);
                        div()
                            .id(SharedString::from(tag.clone()))
                            .tag(tag)
                            .flex_shrink_0()
                            .py(px(3.))
                            .pr(px(6.))
                            .pl(px(match lv {
                                1 => 6.,
                                2 => 16.,
                                _ => 26.,
                            }))
                            .rounded(px(5.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.5))
                            .line_height(relative(1.5))
                            .cursor_pointer()
                            .text_color(if on { k.text } else { k.text_3 })
                            .when(on, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
                            .child(text.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.scroll_to_block(ix);
                                cx.notify();
                            }))
                    })),
            )
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
