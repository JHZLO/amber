// 노트 트리에서 찾기(⌘F) — 지금 앱 TreeFind.tsx. 트리 판에 포커스가 있을 때만 ⌘F 를 받는다(본문의 ⌘F 와 겹치지 않게).
// 찾기 줄은 트리 머리 아래 한 줄로 눕고, 맞은 것과 조상만 남긴 트리를 그린다(조상 폴더는 임시로 펼친다).
// Enter = 지금 것 열기(파일이면 열고 닫는다, 폴더면 펼치기만), ↑↓ = 다음/이전, Esc = 닫기.

use amber_gpui::i18n::t;
use amber_gpui::tree_find;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;
use crate::vault_pane::Which;

pub struct TreeFindState {
    pub input: Entity<InputState>,
    pub query: String,
    pub idx: usize,
}

/// 어느 트리의 찾기인가 — 보관함 트리(노트, 다이어그램) 또는 ERD 트리(연결, 스키마, 표). 줄과 동작은 같다
#[derive(Clone, Copy, PartialEq)]
pub enum FindIn {
    Tree(Which),
    Erd,
}

impl From<Which> for FindIn {
    fn from(w: Which) -> Self {
        FindIn::Tree(w)
    }
}

impl FindIn {
    fn tag(self, base: &str) -> String {
        match self {
            FindIn::Tree(w) => w.tag(base),
            FindIn::Erd => format!("erd-{base}"),
        }
    }
}

impl Spike {
    fn find_of(&self, w: FindIn) -> Option<&TreeFindState> {
        match w {
            FindIn::Tree(w) => self.ui(w).find.as_ref(),
            FindIn::Erd => self.erd.tree_find.as_ref(),
        }
    }

    fn find_slot(&mut self, w: FindIn) -> &mut Option<TreeFindState> {
        match w {
            FindIn::Tree(w) => &mut self.ui_mut(w).find,
            FindIn::Erd => &mut self.erd.tree_find,
        }
    }

    pub(crate) fn start_tree_find(
        &mut self,
        w: impl Into<FindIn>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let w = w.into();
        if let Some(f) = self.find_of(w) {
            // 이미 열려 있으면 글자를 고른 채 다시 잡는다
            f.input.update(cx, |s, cx| s.focus(window, cx));
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t("common.find.treePh")));
        cx.subscribe_in(
            &input,
            window,
            move |this, input, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    let q = input.read(cx).value().to_string();
                    // Enter 뒤에도 값이 그대로인 Change 가 온다 — 검색어가 바뀔 때만 처음으로
                    if let Some(f) = this.find_slot(w).as_mut()
                        && f.query != q
                    {
                        f.query = q;
                        f.idx = 0;
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.tree_find_open(w, cx);
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        *self.find_slot(w) = Some(TreeFindState {
            input,
            query: String::new(),
            idx: 0,
        });
    }

    pub(crate) fn tree_find_matches(&self, w: FindIn) -> Vec<tree_find::Match> {
        let Some(f) = self.find_of(w) else {
            return Vec::new();
        };
        match w {
            FindIn::Tree(w) => self
                .ui(w)
                .tree
                .as_ref()
                .map(|tree| tree_find::filter_tree(tree, &f.query).matches)
                .unwrap_or_default(),
            FindIn::Erd => tree_find::filter_tree(&self.erd_nodes(), &f.query).matches,
        }
    }

    fn tree_find_step(&mut self, w: FindIn, delta: i64) {
        let n = self.tree_find_matches(w).len() as i64;
        if let Some(f) = self.find_slot(w).as_mut()
            && n > 0
        {
            f.idx = (f.idx as i64 + delta).rem_euclid(n) as usize;
        }
    }

    /// Enter — 폴더면 펼치기만(찾기는 그대로), 파일이면 열고 찾기를 닫는다
    fn tree_find_open(&mut self, w: FindIn, cx: &mut Context<Self>) {
        let idx = self.find_of(w).map_or(0, |f| f.idx);
        let Some((path, is_dir)) = self.tree_find_matches(w).get(idx).cloned() else {
            return;
        };
        let w = match w {
            FindIn::Tree(w) => w,
            FindIn::Erd => {
                self.erd_find_pick(&path, is_dir, cx);
                return;
            }
        };
        if is_dir {
            let ui = self.ui_mut(w);
            ui.expanded.extend(amber_gpui::vault::ancestor_paths(&path));
            ui.active_dir = path;
            return;
        }
        self.request_open(w, &path, cx);
        self.ui_mut(w).find = None;
    }

    /// 찾기 줄(.page-find.tree-find) — 돋보기, 입력, 몇 번째/몇 개, 이전, 다음, 닫기
    pub(crate) fn tree_find_bar(
        &self,
        w: impl Into<FindIn>,
        count: usize,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let w = w.into();
        let f = self.find_of(w)?;
        let nav = |id: &'static str, name: &'static str, enabled: bool| {
            let (bg, fg) = (k.surface_2, k.text_2);
            let id = SharedString::from(w.tag(id));
            div()
                .id(id.clone())
                .tag(id.clone())
                .group(id.clone())
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
            format!("{}/{count}", f.idx + 1)
        } else if !f.query.trim().is_empty() {
            "0".into()
        } else {
            String::new()
        };
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .border_b_1()
                .border_color(k.glass_edge)
                .bg(linear_gradient(
                    150.,
                    linear_color_stop(k.glass_fill_hover.0, 0.),
                    linear_color_stop(k.glass_fill_hover.1, 1.),
                ))
                .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                    match e.keystroke.key.as_str() {
                        "escape" => *this.find_slot(w) = None,
                        "down" => this.tree_find_step(w, 1),
                        "up" => this.tree_find_step(w, -1),
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
                .child(icon("search", 14.).flex_shrink_0().text_color(k.text_3))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .child(Input::new(&f.input).appearance(false).px_0()),
                )
                .child(
                    div()
                        .id(SharedString::from(w.tag("tree-find-count")))
                        .tag(w.tag("tree-find-count"))
                        .min_w(px(34.))
                        .flex()
                        .justify_end()
                        .text_size(px(11.5))
                        .text_color(k.text_3)
                        .child(label),
                )
                .child(
                    nav("tree-find-prev", "chevron-up", count > 0).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.tree_find_step(w, -1);
                            cx.notify();
                        },
                    )),
                )
                .child(
                    nav("tree-find-next", "chevron-down", count > 0).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.tree_find_step(w, 1);
                            cx.notify();
                        },
                    )),
                )
                .child(nav("tree-find-close", "x", true).on_click(cx.listener(
                    move |this, _, _, cx| {
                        *this.find_slot(w) = None;
                        cx.notify();
                    },
                ))),
        )
    }
}

/// 라벨 — 찾는 중이면 맞은 글자만 칠한다(회색, 지금 것은 노랑)
pub fn label(name: &str, query: Option<&str>, current: bool, k: &Tokens) -> StyledText {
    let Some(q) = query.filter(|q| !q.trim().is_empty()) else {
        return StyledText::new(name.to_string());
    };
    let mut hl = Vec::new();
    let mut at = 0;
    for (text, hit) in tree_find::split_hit(name, q) {
        let end = at + text.len();
        if hit {
            let bg = match (current, k.dark) {
                (true, true) => hsla(43. / 360., 0.96, 0.40, 0.62),
                (true, false) => hsla(50. / 360., 0.95, 0.53, 0.55),
                (false, true) => hsla(240. / 360., 0.09, 0.66, 0.3),
                (false, false) => hsla(240. / 360., 0.04, 0.49, 0.3),
            };
            hl.push((
                at..end,
                HighlightStyle {
                    background_color: Some(bg),
                    color: current.then_some(k.text),
                    ..Default::default()
                },
            ));
        }
        at = end;
    }
    // 소문자로 바꾸며 길이가 변하는 글자(İ 등)는 조각이 원문과 어긋날 수 있다 — 그땐 칠하지 않는다
    if at != name.len()
        || hl
            .iter()
            .any(|(r, _)| !name.is_char_boundary(r.start) || !name.is_char_boundary(r.end))
    {
        return StyledText::new(name.to_string());
    }
    StyledText::new(name.to_string()).with_highlights(hl)
}
