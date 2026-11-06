// 설정 모달 — 지금 앱 SettingsModal.tsx 의 틀. 넓게(980) 열고 왼쪽에 칸 목록(AI, 프롬프트, 데일리 리포트,
// 데이터베이스, 모양), 오른쪽 판이 제 안에서 구른다. 열 때마다 AI 칸으로 돌아간다(지금 앱과 같다).
// 칸 내용은 하나씩 옮긴다 — 리포트(D5d)가 먼저, 나머지는 D7.

use std::rc::Rc;

use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button};
use crate::report_probe::{Core, Fake, ReportProbe};
use crate::report_settings::RepSettings;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Ai,
    Prompts,
    Report,
    Databases,
    Appearance,
}

const TABS: [(Tab, &str, &str, &str); 5] = [
    (Tab::Ai, "set-tab-ai", "settings.tab.ai", "sparkles"),
    (
        Tab::Prompts,
        "set-tab-prompts",
        "settings.tab.prompts",
        "lightbulb",
    ),
    (
        Tab::Report,
        "set-tab-report",
        "settings.tab.report",
        "calendar-check",
    ),
    (
        Tab::Databases,
        "set-tab-databases",
        "settings.tab.databases",
        "database",
    ),
    (
        Tab::Appearance,
        "set-tab-appearance",
        "settings.tab.appearance",
        "sun",
    ),
];

pub struct SettingsUi {
    pub open: bool,
    pub tab: Tab,
    /// 리포트 칸 — 칸이 처음 보일 때 만든다(입력칸은 창이 있어야 만들어진다), 닫으면 버린다
    pub rep: Option<RepSettings>,
    /// gh, claude 확인 — 시험 빌드는 늘 가짜(사용자의 gh, claude 에 닿지 않게)
    pub probe: Rc<dyn ReportProbe>,
    /// 오른쪽 판의 스크롤 — 칸을 바꾸면 맨 위로
    pub pane: ScrollHandle,
}

impl Default for SettingsUi {
    fn default() -> Self {
        SettingsUi {
            open: false,
            tab: Tab::Ai,
            rep: None,
            probe: if cfg!(test) {
                Rc::new(Fake::default())
            } else {
                Rc::new(Core)
            },
            pane: ScrollHandle::new(),
        }
    }
}

impl Spike {
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.open = true;
        self.settings.tab = Tab::Ai;
        self.settings.rep = None;
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.open = false;
        self.settings.rep = None;
        cx.notify();
    }

    /// 그리기 전에 — 보이는 칸의 상태를 만든다(창이 필요하다)
    pub(crate) fn prepare_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.open && self.settings.tab == Tab::Report && self.settings.rep.is_none() {
            self.mount_report_settings(window, cx);
        }
        self.sync_report_settings(window, cx);
    }

    pub(crate) fn settings_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if !self.settings.open {
            return None;
        }
        let cur = self.settings.tab;
        let (hover_fill, hover_edge, hover_shadow) =
            (glass(k.glass_fill), k.glass_edge, k.glass_shadow(false));
        let text = k.text;
        let nav = div()
            .flex_none()
            .w(px(196.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(14.))
            .px(px(10.))
            .border_r_1()
            .border_color(k.border)
            .children(TABS.iter().map(|&(tab, id, label, name)| {
                let active = tab == cur;
                let (hf, he, hs) = (hover_fill.clone(), hover_edge, hover_shadow.clone());
                div()
                    .id(id)
                    .tag(id)
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .py(px(8.))
                    .px(px(10.))
                    .rounded(px(8.))
                    .border_1()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .when(active, |d| {
                        d.bg(glass(k.glass_fill_accent))
                            .border_color(k.glass_edge_accent)
                            .shadow(k.glass_shadow(true))
                            .text_color(k.text)
                            .child(icon(name, 15.).text_color(k.text_2))
                    })
                    .when(!active, |d| {
                        d.border_color(transparent_black())
                            .text_color(k.text_2)
                            .hover(move |s| s.bg(hf).border_color(he).shadow(hs).text_color(text))
                            .child(icon(name, 15.).text_color(k.text_3))
                    })
                    .child(t(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.tab = tab;
                        this.settings.pane.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    }))
            }));
        let pane = div()
            .id("set-pane")
            .tag("set-pane")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings.pane)
            .pt(px(18.))
            .px(px(22.))
            .pb(px(22.))
            .when(cur == Tab::Report, |d| d.child(self.report_settings(k, cx)));
        let body = div().size_full().flex().child(nav).child(pane);
        Some(crate::dialog::shell_settings(
            k,
            t("settings.title"),
            body.into_any_element(),
            vec![
                button("set-close", k, false, false)
                    .child(t("common.close"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_settings(cx))),
            ],
            cx.listener(|this, _, _, cx| this.close_settings(cx)),
        ))
    }
}

pub(crate) fn glass(pair: (Hsla, Hsla)) -> Background {
    linear_gradient(
        150.,
        linear_color_stop(pair.0, 0.),
        linear_color_stop(pair.1, 1.),
    )
}

/// .set-section 의 머리 — 작은 대문자 제목 | 빈칸 | 동작 하나, 그 아래 설명
pub(crate) fn set_section(
    k: &Tokens,
    title: String,
    desc: Option<String>,
    action: Option<AnyElement>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(26.))
                .mb(px(10.))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text_3)
                        .child(title.to_uppercase()),
                )
                .child(div().flex_1())
                .children(action),
        )
        .when_some(desc, |d, desc| {
            d.child(
                div()
                    .mb(px(12.))
                    .max_w(px(470.))
                    .text_size(px(12.5))
                    .line_height(relative(1.6))
                    .text_color(k.text_3)
                    .child(desc),
            )
        })
}
