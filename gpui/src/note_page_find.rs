// 노트 안에서 찾기(⌘F) — 지금 앱 PageFind.tsx, lib/pageFind.ts. 그려진 글자에서 찾아 칠한다(맞은 것 회색, 지금 것 노랑).
// 글을 고치지 않고 배경만 칠한다(TextView 의 RangeHighlight). Enter = 다음, Shift+Enter = 이전, Esc = 닫기.
// 찾기 줄은 본문 판 오른쪽 위에 떠서 본문 폭을 밀지 않는다. 트리에 포커스가 있을 때의 ⌘F 는 트리 찾기다.
// 아직: 알림 상자와 코드 창 안의 글자(직접 그리는 블록), 편집 중의 원문 칸 찾기.

use std::ops::Range;

use amber_gpui::i18n::t;
use gpui_kit::base::text::RangeHighlight;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::notes_view::Block;
use crate::theme::Tokens;

/// 한 번에 칠할 상한 — 넘는 만큼은 개수에 '+'로만 알린다
const FIND_LIMIT: usize = 500;

pub struct PageFindState {
    pub input: Entity<InputState>,
    pub query: String,
    pub idx: usize,
    /// (본문 덩어리 번호, 그려진 글자의 바이트 범위)
    pub hits: Vec<(usize, Range<usize>)>,
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
        if self.notes.selected.is_none() || self.notes.edit.is_some() {
            return;
        }
        if let Some(f) = self.notes.page_find.as_ref() {
            f.input.update(cx, |s, cx| s.focus(window, cx));
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
                    this.paint_page_find(cx);
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
        });
    }

    pub(crate) fn close_page_find(&mut self, cx: &mut Context<Self>) {
        self.notes.page_find = None;
        for b in &self.notes.blocks {
            if let Block::Text(state) = b {
                state.update(cx, |s, cx| s.clear_range_highlights(cx));
            }
        }
    }

    fn page_find_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(f) = self.notes.page_find.as_mut() {
            let n = f.hits.len() as i64;
            if n == 0 {
                return;
            }
            f.idx = (f.idx as i64 + delta).rem_euclid(n) as usize;
        }
        self.paint_page_find(cx);
    }

    /// 덩어리마다 다시 찾고 칠한다. 지금 것은 그 줄로 굴린다
    pub(crate) fn paint_page_find(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.notes.page_find.as_ref() else {
            return;
        };
        let query = f.query.clone();
        let idx = f.idx;
        let dark = crate::theme::Tokens::get(cx).dark;
        // 질문 형광펜과 구분되는 색 — 맞은 것은 회색, 지금 것은 노랑(styles.css 의 ::highlight(page-find))
        let (hit, cur) = if dark {
            (
                hsla(240. / 360., 0.09, 0.66, 0.3),
                hsla(43. / 360., 0.96, 0.40, 0.62),
            )
        } else {
            (
                hsla(240. / 360., 0.04, 0.49, 0.3),
                hsla(50. / 360., 0.95, 0.53, 0.55),
            )
        };
        let mut hits = Vec::new();
        let mut n = 0usize;
        for (bi, b) in self.notes.blocks.iter().enumerate() {
            let Block::Text(state) = b else {
                continue;
            };
            let text = state.read(cx).rendered_text();
            let ranges = find_in_text(
                text.as_str(),
                &query,
                FIND_LIMIT - hits.len().min(FIND_LIMIT),
            );
            let marks: Vec<RangeHighlight> = ranges
                .iter()
                .map(|r| {
                    let me = n == idx;
                    n += 1;
                    RangeHighlight::new(r.clone(), if me { cur } else { hit })
                })
                .collect();
            hits.extend(ranges.into_iter().map(|r| (bi, r)));
            state.update(cx, |s, cx| {
                let _ = s.set_range_highlights(marks, cx);
            });
        }
        if let Some((bi, r)) = hits.get(idx).cloned()
            && let Some(Block::Text(state)) = self.notes.blocks.get(bi)
        {
            state.update(cx, |s, cx| {
                let _ = s.reveal_range(r, cx);
            });
        }
        if let Some(f) = self.notes.page_find.as_mut() {
            f.hits = hits;
        }
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
        let count = f.hits.len();
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
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                    if e.keystroke.key == "escape" {
                        this.close_page_find(cx);
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
                    |this, _, _, cx| {
                        this.close_page_find(cx);
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
