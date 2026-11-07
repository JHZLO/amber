// 노트 안에서 찾기(⌘F) — 지금 앱 PageFind.tsx, lib/pageFind.ts. 그려진 글자에서 찾아 칠한다(맞은 것 회색, 지금 것 노랑).
// 글을 고치지 않고 배경만 칠한다(TextView 의 RangeHighlight). Enter = 다음, Shift+Enter = 이전, Esc = 닫기.
// 찾기 줄은 본문 판 오른쪽 위에 떠서 본문 폭을 밀지 않는다. 트리에 포커스가 있을 때의 ⌘F 는 트리 찾기다.
// 편집 중에는 원문에서 찾는다(지금 앱의 sourceRef 갈래) — 미리보기는 칠하지 않는다(마크다운 기호 때문에 개수가 어긋난다).
// 알림 상자와 코드 창(직접 그리는 블록)은 블록이 자기 조각을 적어 두고 찾기가 그 조각에서 찾아 칠을 넘긴다(markdown::CustomFind).

use std::ops::Range;

use amber_gpui::i18n::t;
use gpui_kit::component::input::{BufferPoint, Input, InputEvent, InputState, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::markdown::{CustomFind, Piece};
use crate::notes_view::text_states;
use crate::theme::Tokens;
use gpui_kit::base::text::{RangeHighlight, TextViewState};

/// 한 번에 칠할 상한 — 넘는 만큼은 개수에 '+'로만 알린다
const FIND_LIMIT: usize = 500;

pub struct PageFindState {
    pub input: Entity<InputState>,
    pub query: String,
    pub idx: usize,
    /// (글 덩어리 번호 — text_states 순서, 토글 안까지, 그려진 글자의 바이트 범위)
    pub hits: Vec<(usize, Range<usize>)>,
    /// 편집 중의 결과 — 원문(초안)의 바이트 범위, 그리고 그걸 찾은 원문
    pub src: Vec<Range<usize>>,
    pub src_text: String,
    /// 지금 결과를 어느 쪽에서 찾았나(편집 = 원문). 읽기와 편집을 오가면 그 자리에서 다시 찾는다
    pub in_src: bool,
    /// 나갈 때 커서를 둘 자리 — 마지막으로 보여 준 원문 구간
    pub last: Option<Range<usize>>,
}

/// 찾기의 칠 — 맞은 것 회색, 지금 것 노랑(styles.css 의 ::highlight(page-find), .find-src-mirror mark)
fn find_colors(dark: bool) -> (Hsla, Hsla) {
    if dark {
        (
            hsla(240. / 360., 0.09, 0.66, 0.3),
            hsla(43. / 360., 0.96, 0.40, 0.62),
        )
    } else {
        (
            hsla(240. / 360., 0.04, 0.49, 0.3),
            hsla(50. / 360., 0.95, 0.53, 0.55),
        )
    }
}

/// 결과 하나를 칠할 곳 — 글 덩어리의 그려진 글, 직접 그리는 블록의 글 조각(블록 번호, 조각 번호), 알림 본문 보기
enum At {
    Text(usize, Range<usize>),
    Piece(usize, usize, Range<usize>),
    View(Entity<TextViewState>, Range<usize>),
}

/// 직접 그리는 블록의 칠을 걷는다(다시 찾기 전, 닫을 때)
fn clear_custom_marks(cx: &mut App) {
    let views: Vec<Entity<TextViewState>> = {
        let reg = cx.default_global::<CustomFind>();
        for b in reg.0.iter_mut() {
            b.marks.clear();
        }
        reg.0
            .iter()
            .flat_map(|b| b.pieces.iter())
            .filter_map(|p| match p {
                Piece::View(v) => Some(v.clone()),
                Piece::Text(_) => None,
            })
            .collect()
    };
    for v in views {
        v.update(cx, |s, cx| s.clear_range_highlights(cx));
    }
}

/// 원문 칸 위에 찾기 칠을 덧그린다. 칸이 이번 그림에서 잰 자리(text_bounds)를 쓰려고 칸 **뒤에** 그리고, 칠한
/// 글자는 그 위에 다시 쓴다 — 지금 앱은 거울판을 글자 밑에 깔아 글자 색이 칠에 물들지 않는다.
/// 칠은 글자 높이만(<mark> 의 줄 상자), 보이는 줄의 결과만, 접혀 넘어간 결과는 화면 줄마다 한 조각
pub(crate) fn paint_src_find(
    state: &Entity<TextareaState>,
    hits: &[Range<usize>],
    cur: usize,
    dark: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let (hit_c, cur_c) = find_colors(dark);
    let s = state.read(cx);
    let (Some(rows), Some(lh), Some(tb)) =
        (s.visible_row_range(), s.line_height(), s.text_bounds())
    else {
        return;
    };
    let text = s.value();
    let dm = s.display_map();
    // 결과는 앞에서부터 차례라 줄 번호를 이어서 센다(찾기 줄은 한 줄이라 결과 안에 줄바꿈이 없다)
    let (mut line, mut line_start, mut at) = (0usize, 0usize, 0usize);
    let mut pieces: Vec<(Range<usize>, bool)> = Vec::new();
    for (i, r) in hits.iter().enumerate() {
        // 찾은 뒤 원문이 바뀐 그림 — 다음 그림에서 다시 찾는다
        if r.end > text.len() || !text.is_char_boundary(r.start) || !text.is_char_boundary(r.end) {
            break;
        }
        for (j, ch) in text[at..r.start].char_indices() {
            if ch == '\n' {
                line += 1;
                line_start = at + j + 1;
            }
        }
        at = r.start;
        let row = |off: usize| {
            dm.buffer_pos_to_display_pos(BufferPoint::new(line, off - line_start))
                .row
        };
        if row(r.start) >= rows.end {
            break;
        }
        let mut seg_start = r.start;
        let mut seg_row = row(r.start);
        for (j, _) in text[r.clone()].char_indices().skip(1) {
            let row_j = row(r.start + j);
            if row_j != seg_row {
                if rows.contains(&seg_row) {
                    pieces.push((seg_start..r.start + j, i == cur));
                }
                (seg_start, seg_row) = (r.start + j, row_j);
            }
        }
        if rows.contains(&seg_row) {
            pieces.push((seg_start..r.end, i == cur));
        }
    }
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let font_id = window.text_system().resolve_font(&style.font());
    let glyph_h = window.text_system().ascent(font_id, font_size)
        + window.text_system().descent(font_id, font_size).abs();
    for (r, is_cur) in pieces {
        let u16s = text[..r.start].encode_utf16().count();
        let u16r = u16s..u16s + text[r.clone()].encode_utf16().count();
        let Some(b) = state.update(cx, |s, cx| s.bounds_for_range(u16r, tb, window, cx)) else {
            continue;
        };
        let rect = Bounds::new(
            point(b.left(), b.top() + (lh - glyph_h) / 2.),
            size(b.size.width, glyph_h),
        );
        window.paint_quad(fill(rect, if is_cur { cur_c } else { hit_c }).corner_radii(px(2.)));
        let shaped = window.text_system().shape_line(
            SharedString::from(text[r.clone()].to_string()),
            font_size,
            &[style.to_run(r.len())],
            None,
        );
        let _ = shaped.paint(b.origin, lh, TextAlign::Left, None, window, cx);
    }
}

/// pageFind.ts 의 findInText — 대소문자 무시. 소문자로 바꾸며 길이가 변하는 글자(İ 등)가 섞이면 원문으로 찾는다
pub fn find_in_text(text: &str, query: &str, limit: usize) -> Vec<Range<usize>> {
    let q = query.trim();
    if q.is_empty() || limit == 0 {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let (hay, needle) = if lower.len() == text.len() {
        (lower, q.to_lowercase())
    } else {
        (text.to_string(), q.to_string())
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let at = from + i;
        out.push(at..at + needle.len());
        if out.len() >= limit {
            break;
        }
        from = at + needle.len();
    }
    out
}

impl Spike {
    pub(crate) fn start_page_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notes.selected.is_none() {
            return;
        }
        // 이미 열려 있으면 칸으로 돌아가 친 글을 고른다 — 바로 새 검색어를 칠 수 있게
        if let Some(f) = self.notes.page_find.as_ref() {
            f.input.update(cx, |s, cx| {
                s.focus(window, cx);
                s.select_all(window, cx);
            });
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t("common.find.ph")));
        cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    let q = input.read(cx).value().to_string();
                    // Enter 뒤에도 값이 그대로인 Change 가 온다 — 검색어가 바뀔 때만 처음으로
                    if let Some(f) = this.notes.page_find.as_mut() {
                        if f.query == q {
                            return;
                        }
                        f.query = q;
                        f.idx = 0;
                    }
                    if this.notes.edit.is_some() {
                        this.run_src_find(cx);
                    } else {
                        this.paint_page_find(cx);
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    this.page_find_step(if *shift { -1 } else { 1 }, cx);
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.notes.page_find = Some(PageFindState {
            input,
            query: String::new(),
            idx: 0,
            hits: Vec::new(),
            src: Vec::new(),
            src_text: String::new(),
            in_src: self.notes.edit.is_some(),
            last: None,
        });
    }

    /// 닫기. 편집 중이었고 찾기 줄에 손이 있었으면 찾던 자리에 커서를 두고 나간다 — 찾은 이유가 거기를
    /// 고치려는 것이다. 보던 자리는 그대로 둔다
    pub(crate) fn close_page_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.notes.page_find.take() else {
            return;
        };
        if let (Some(e), Some(at)) = (self.notes.edit.as_ref(), f.last)
            && f.input
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        {
            e.state.update(cx, |s, cx| {
                let keep = s.scroll_offset();
                s.focus(window, cx);
                s.set_selected_range(at, cx);
                s.set_scroll_offset(keep, cx);
            });
        }
        // 질문 형광펜은 남긴다
        self.repaint_marks(cx);
    }

    fn page_find_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(f) = self.notes.page_find.as_mut() {
            let n = if f.in_src { f.src.len() } else { f.hits.len() } as i64;
            if n == 0 {
                return;
            }
            f.idx = (f.idx as i64 + delta).rem_euclid(n) as usize;
            if f.in_src {
                f.last = f.src.get(f.idx).cloned();
            }
        }
        if self.notes.edit.is_some() {
            self.scroll_src_to_cur(cx);
        } else {
            self.paint_page_find(cx);
        }
    }

    /// 원문에서 다시 찾는다(검색어가 바뀌거나, 원문을 고치거나, 편집으로 들어왔을 때) — 처음 결과부터
    pub(crate) fn run_src_find(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.note_draft(cx) else {
            return;
        };
        let Some(f) = self.notes.page_find.as_mut() else {
            return;
        };
        f.src = find_in_text(&draft, &f.query, FIND_LIMIT);
        f.src_text = draft;
        f.idx = 0;
        f.in_src = true;
        f.last = f.src.first().cloned();
        self.scroll_src_to_cur(cx);
    }

    /// 그리기 전에 부른다 — 읽기와 편집을 오갔거나 원문이 바뀌었으면(타이핑, 디스크에서 다시 읽기, AI 적용 —
    /// 뒤쪽 자리가 전부 밀린다) 열린 찾기를 그 자리에서 다시 찾는다
    pub(crate) fn sync_page_find(&mut self, cx: &mut Context<Self>) {
        let editing = self.notes.edit.as_ref().map(|e| e.state.clone());
        let Some(f) = self.notes.page_find.as_mut() else {
            return;
        };
        let changed = editing
            .as_ref()
            .is_some_and(|s| f.in_src && s.read(cx).value().as_ref() != f.src_text);
        if f.in_src == editing.is_some() && !changed {
            return;
        }
        let editing = editing.is_some();
        f.idx = 0;
        if editing {
            self.run_src_find(cx);
        } else {
            f.in_src = false;
            f.src.clear();
            f.last = None;
            self.paint_page_find(cx);
        }
    }

    /// 지금 결과를 원문 칸 안으로 — 이미 보이면 건드리지 않는다(옮길 때마다 튀면 어디로 갔는지 놓친다).
    /// 안 보이면 칸 가운데로
    fn scroll_src_to_cur(&mut self, cx: &mut Context<Self>) {
        let (Some(f), Some(e)) = (self.notes.page_find.as_ref(), self.notes.edit.as_ref()) else {
            return;
        };
        let Some(r) = f.src.get(f.idx).cloned() else {
            return;
        };
        e.state.update(cx, |s, cx| {
            let (Some(lh), Some(tb)) = (s.line_height(), s.text_bounds()) else {
                return;
            };
            let text = s.value();
            let line = text[..r.start].matches('\n').count();
            let ls = text[..r.start].rfind('\n').map_or(0, |i| i + 1);
            let row = s
                .display_map()
                .buffer_pos_to_display_pos(BufferPoint::new(line, r.start - ls))
                .row;
            let top = lh * row as f32;
            let (off, edge, view) = (s.scroll_offset(), px(24.), tb.size.height);
            let seen = top >= -off.y + edge && top + lh <= -off.y + view - edge;
            if !seen {
                let y = (top - view / 2.).max(px(0.));
                s.set_scroll_offset(point(off.x, -y), cx);
            }
        });
    }

    /// 덩어리마다 다시 찾아 칠한다(질문 형광펜과 한 번에). 지금 것은 그 줄로 굴린다
    pub(crate) fn paint_page_find(&mut self, cx: &mut Context<Self>) {
        self.repaint_marks(cx);
        let Some(f) = self.notes.page_find.as_ref() else {
            return;
        };
        if let Some((bi, r)) = f.hits.get(f.idx).cloned()
            && let Some(state) = text_states(&self.notes.blocks).get(bi)
        {
            state.update(cx, |s, cx| {
                let _ = s.reveal_range(r, cx);
            });
        }
    }

    /// 찾기의 칠 — 맞은 것은 회색, 지금 것은 노랑(styles.css 의 ::highlight(page-find)). 맞은 목록도 새로 단다
    pub(crate) fn page_find_marks(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<(usize, Range<usize>, Hsla)> {
        clear_custom_marks(cx);
        let Some(f) = self.notes.page_find.as_ref() else {
            return Vec::new();
        };
        if f.in_src {
            return Vec::new();
        }
        let (query, idx) = (f.query.clone(), f.idx);
        let (hit, cur) = find_colors(Tokens::get(cx).dark);
        // 직접 그리는 블록 — (바깥 글 덩어리, 원문 자리, 조각)
        let customs: Vec<(EntityId, Range<usize>, Vec<Piece>)> = cx
            .default_global::<CustomFind>()
            .0
            .iter()
            .map(|b| {
                let pieces = b
                    .pieces
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => Piece::Text(t.clone()),
                        Piece::View(v) => Piece::View(v.clone()),
                    })
                    .collect();
                (b.outer.entity_id(), b.source.clone(), pieces)
            })
            .collect();
        // 결과마다 (덩어리 번호, 굴릴 자리, 칠할 곳) — 화면 순서대로. 블록 안의 결과는 블록째 굴린다
        let mut found: Vec<(usize, Range<usize>, At)> = Vec::new();
        for (bi, state) in text_states(&self.notes.blocks).iter().enumerate() {
            let text = state.read(cx).rendered_text();
            let mut objs: Vec<(Range<usize>, usize)> = customs
                .iter()
                .enumerate()
                .filter(|(_, c)| c.0 == state.entity_id())
                .filter_map(|(i, c)| text.range_for_source(c.1.clone()).map(|o| (o, i)))
                .collect();
            objs.sort_by_key(|(o, _)| o.start);
            let mut outer = find_in_text(text.as_str(), &query, FIND_LIMIT)
                .into_iter()
                .peekable();
            for (obj, i) in objs {
                while let Some(r) = outer.next_if(|r| r.start < obj.start) {
                    found.push((bi, r.clone(), At::Text(bi, r)));
                }
                // 블록의 글은 블록이 칠한다 — 바깥 글에서 잡힌 것은 버리고 조각에서 다시 찾는다
                while outer.next_if(|r| r.start < obj.end).is_some() {}
                for (pi, piece) in customs[i].2.iter().enumerate() {
                    match piece {
                        Piece::Text(t) => {
                            for r in find_in_text(t, &query, FIND_LIMIT) {
                                found.push((bi, obj.clone(), At::Piece(i, pi, r)));
                            }
                        }
                        Piece::View(v) => {
                            let inner = v.read(cx).rendered_text();
                            for r in find_in_text(inner.as_str(), &query, FIND_LIMIT) {
                                found.push((bi, obj.clone(), At::View(v.clone(), r)));
                            }
                        }
                    }
                }
            }
            found.extend(outer.map(|r| (bi, r.clone(), At::Text(bi, r))));
            if found.len() >= FIND_LIMIT {
                break;
            }
        }
        found.truncate(FIND_LIMIT);
        let mut marks = Vec::new();
        let mut piece_marks: Vec<(usize, usize, Range<usize>, Hsla)> = Vec::new();
        let mut view_marks: Vec<(Entity<TextViewState>, RangeHighlight)> = Vec::new();
        for (n, (_, _, at)) in found.iter().enumerate() {
            let color = if n == idx { cur } else { hit };
            match at {
                At::Text(bi, r) => marks.push((*bi, r.clone(), color)),
                At::Piece(i, pi, r) => piece_marks.push((*i, *pi, r.clone(), color)),
                At::View(v, r) => {
                    view_marks.push((v.clone(), RangeHighlight::new(r.clone(), color)))
                }
            }
        }
        {
            let reg = cx.default_global::<CustomFind>();
            for (i, pi, r, c) in piece_marks {
                if let Some(b) = reg.0.get_mut(i) {
                    b.marks.push((pi, r, c));
                }
            }
        }
        let mut per_view: Vec<(Entity<TextViewState>, Vec<RangeHighlight>)> = Vec::new();
        for (v, h) in view_marks {
            match per_view.iter_mut().find(|(w, _)| *w == v) {
                Some((_, hs)) => hs.push(h),
                None => per_view.push((v, vec![h])),
            }
        }
        for (v, hs) in per_view {
            v.update(cx, |s, cx| {
                let _ = s.set_range_highlights(hs, cx);
            });
        }
        if let Some(f) = self.notes.page_find.as_mut() {
            f.hits = found.into_iter().map(|(bi, r, _)| (bi, r)).collect();
        }
        marks
    }

    /// TextView 가 그 줄을 보여 달라고 했다 — 판 밖이면 판 위에서 3분의 1 자리로 굴린다
    pub(crate) fn reveal_in_note(&mut self, line: Bounds<Pixels>) {
        let view = self.scroll.bounds();
        if line.top() >= view.top() + px(60.) && line.bottom() <= view.bottom() - px(20.) {
            return;
        }
        let off = self.scroll.offset();
        let y = (off.y - (line.top() - view.top()) + view.size.height / 3.).min(px(0.));
        self.scroll.set_offset(point(off.x, y));
    }

    /// 찾기 줄(.page-find) — 본문 판 오른쪽 위에 뜬다
    pub(crate) fn page_find_bar(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let f = self.notes.page_find.as_ref()?;
        let count = if f.in_src { f.src.len() } else { f.hits.len() };
        let nav = |id: &'static str, name: &'static str, enabled: bool| {
            let (bg, fg) = (k.surface_2, k.text_2);
            div()
                .id(id)
                .tag(id)
                .group(id)
                .flex()
                .p(px(3.))
                .rounded(px(5.))
                .when(!enabled, |d| d.opacity(0.5))
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(bg)))
                .child(
                    icon(name, 14.)
                        .text_color(k.text_3)
                        .group_hover(id, move |s| s.text_color(fg)),
                )
        };
        let label = if count > 0 {
            format!(
                "{}/{count}{}",
                f.idx + 1,
                if count >= FIND_LIMIT { "+" } else { "" }
            )
        } else if !f.query.trim().is_empty() {
            "0".into()
        } else {
            String::new()
        };
        Some(
            div()
                .absolute()
                .top(px(8.))
                .right(px(12.))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(9.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .shadow(k.shadow())
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                    if e.keystroke.key == "escape" {
                        this.close_page_find(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .child(icon("search", 14.).text_color(k.text_3))
                .child(
                    div()
                        .w(px(200.))
                        .text_size(px(13.))
                        .child(Input::new(&f.input).appearance(false).px_0()),
                )
                .child(
                    div()
                        .id("page-find-count")
                        .tag("page-find-count")
                        .min_w(px(34.))
                        .flex()
                        .justify_end()
                        .text_size(px(11.5))
                        .text_color(k.text_3)
                        .child(label),
                )
                .child(
                    nav("page-find-prev", "chevron-up", count > 0).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.page_find_step(-1, cx);
                            cx.notify();
                        },
                    )),
                )
                .child(
                    nav("page-find-next", "chevron-down", count > 0).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.page_find_step(1, cx);
                            cx.notify();
                        },
                    )),
                )
                .child(nav("page-find-close", "x", true).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.close_page_find(window, cx);
                        cx.notify();
                    },
                ))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::find_in_text;

    #[test]
    fn like_page_find_in_text() {
        assert_eq!(
            find_in_text("Abc abc ABC", " abc ", 500),
            vec![0..3, 4..7, 8..11]
        );
        assert_eq!(
            find_in_text("aaaa", "aa", 500),
            vec![0..2, 2..4],
            "겹치지 않게"
        );
        assert_eq!(find_in_text("한글 한글", "한", 1), vec![0..3], "상한");
        // İ 는 소문자가 되며 길이가 바뀐다 — 원문으로 찾는다(대소문자 구분)
        assert_eq!(find_in_text("İx ix", "ix", 500), vec![4..6]);
        assert!(find_in_text("abc", "  ", 500).is_empty());
    }
}
