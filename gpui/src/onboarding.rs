// 첫 실행 안내 — 지금 앱 AiOnboarding.tsx. 연결도 건너뛰기도 하지 않은 처음 한 번만 뜬다.
// 로그인 셸 PATH 에서 설치된 AI CLI 를 찾아 카드로(첫 것을 미리 골라 둔다), [연결] 한 번이면 끝.
// 못 찾으면 설치 안내 단추. 닫기와 [AI 없이 사용]은 같은 일이다(다시 묻지 않는다).

use amber_gpui::ai_config;
use amber_gpui::i18n::{t, t_with};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::note_ai_view::hint;
use crate::report_probe::Cli;
use crate::settings_ai::provider_label;
use crate::settings_view::glass;
use crate::theme::Tokens;

const INSTALL_LINKS: [(&str, &str); 2] = [
    ("claude", "https://claude.com/claude-code"),
    ("codex", "https://developers.openai.com/codex"),
];

pub struct Onboarding {
    pub detecting: bool,
    pub detected: Vec<Cli>,
    pub selected: Option<String>,
    pub busy: bool,
}

impl Spike {
    /// 처음이면(연결도 건너뛰기도 안 했으면) 안내를 띄우고 감지를 돌린다
    pub(crate) fn maybe_onboard(&mut self, cx: &mut Context<Self>) {
        let done = ai_config::load_config(&self.db).is_ok_and(|c| c.onboarded);
        if done {
            return;
        }
        self.onboarding = Some(Onboarding {
            detecting: true,
            detected: Vec::new(),
            selected: None,
            busy: false,
        });
        let rx = self.settings.probe.ai_clis();
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let list = crate::ai::wait(rx, ex).await.unwrap_or_default();
            let _ = this.update(cx, |s, cx| {
                if let Some(o) = s.onboarding.as_mut() {
                    o.selected = list.first().map(|c| c.id.clone());
                    o.detected = list;
                    o.detecting = false;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn onboard_connect(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.onboarding.as_ref() else {
            return;
        };
        let Some(cli) = o
            .selected
            .as_ref()
            .and_then(|id| o.detected.iter().find(|c| c.id == *id))
            .cloned()
        else {
            return;
        };
        let _ = ai_config::connect_provider(&self.db, &cli.id, &cli.path, None);
        self.onboarding = None;
        cx.notify();
    }

    /// 건너뛰기 — AI 없이 쓴다(설정에서 언제든 연결)
    pub(crate) fn onboard_skip(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.as_ref().is_some_and(|o| o.busy) {
            return;
        }
        let _ = ai_config::skip_ai_onboarding(&self.db);
        self.onboarding = None;
        cx.notify();
    }

    pub(crate) fn onboarding_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let o = self.onboarding.as_ref()?;
        let lead_pre = t("settings.onb.lead.pre");
        let lead_bold = t("settings.onb.lead.bold");
        let lead = format!("{lead_pre}{lead_bold}{}", t("settings.onb.lead.post"));
        let bold = lead_pre.len()..lead_pre.len() + lead_bold.len();
        let mut body = div().flex().flex_col().child(
            div()
                .mb(px(14.))
                .text_size(px(13.5))
                .line_height(relative(1.7))
                .text_color(k.text_2)
                .child(StyledText::new(lead).with_highlights(vec![(
                    bold,
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                )])),
        );
        if o.detecting {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(40.))
                    .text_color(k.text_2)
                    .child(crate::spinner::spinner(k, cx.reduce_motion()))
                    .child(t("settings.ai.searching"))
                    .child(hint(t("settings.onb.searchHint"), k).mt_0()),
            );
        } else if o.detected.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .mb(px(10.))
                            .text_size(px(13.))
                            .line_height(relative(1.7))
                            .text_color(k.text_2)
                            .child(t("settings.onb.emptyTitle"))
                            .child(t("settings.onb.emptyBody")),
                    )
                    .child(div().flex().flex_wrap().gap(px(6.)).children(
                        INSTALL_LINKS.iter().map(|&(p, url)| {
                            button(
                                SharedString::from(format!("onb-install-{p}")),
                                k,
                                false,
                                false,
                            )
                            .child(t_with(
                                "settings.onb.installGuide",
                                &[("name", provider_label(p))],
                            ))
                            .on_click(move |_, _, cx| cx.open_url(url))
                        }),
                    )),
            );
        } else {
            let mut grid = div().flex().flex_col().gap(px(8.));
            for d in &o.detected {
                let checked = o.selected.as_deref() == Some(d.id.as_str());
                let (hf, he) = (glass(k.glass_fill_hover), k.glass_edge_hover);
                let id = d.id.clone();
                grid = grid.child(
                    div()
                        .id(SharedString::from(format!("onb-card-{}", d.id)))
                        .tag(format!("onb-card-{}", d.id))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .py(px(11.))
                        .px(px(13.))
                        .rounded(px(10.))
                        .border_1()
                        .cursor_pointer()
                        .when(checked, |e| {
                            e.bg(glass(k.glass_fill_accent))
                                .border_color(k.glass_edge_accent)
                                .shadow(k.glass_shadow(true))
                        })
                        .when(!checked, |e| {
                            e.bg(glass(k.glass_fill))
                                .border_color(k.glass_edge)
                                .shadow(k.glass_shadow(false))
                                .hover(move |s| s.bg(hf).border_color(he))
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(9.))
                                .child(
                                    div()
                                        .size(px(14.))
                                        .flex_none()
                                        .rounded_full()
                                        .border(px(1.5))
                                        .when(checked, |e| e.border_color(k.text).bg(k.text))
                                        .when(!checked, |e| e.border_color(k.border_strong)),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.5))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(if checked { k.text } else { k.text_2 })
                                        .child(d.name.clone()),
                                )
                                .child(
                                    div()
                                        .font_family(crate::markdown::MONO)
                                        .text_size(px(11.5))
                                        .text_color(k.text_3)
                                        .child(d.version.clone()),
                                ),
                        )
                        .child(
                            div()
                                .pl(px(23.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(11.))
                                .text_color(k.text_3)
                                .child(d.path.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(o) = this.onboarding.as_mut() {
                                o.selected = Some(id.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            body = body.child(grid);
        }
        body = body.child(hint(t("settings.onb.laterHint"), k).mt(px(12.)));
        let off = o.busy || o.selected.is_none() || o.detecting;
        Some(crate::dialog::shell_split(
            k,
            640.,
            t("settings.ai.title"),
            body.into_any_element(),
            vec![
                button("onb-skip", k, false, o.busy)
                    .child(t("settings.onb.skip"))
                    .on_click(cx.listener(|this, _, _, cx| this.onboard_skip(cx))),
            ],
            vec![
                primary_button("onb-connect", k, off)
                    .child(icon("sparkles", 15.).text_color(k.text))
                    .child(t(if o.busy {
                        "settings.onb.connecting"
                    } else {
                        "settings.onb.connect"
                    }))
                    .when(!off, |b| {
                        b.on_click(cx.listener(|this, _, _, cx| this.onboard_connect(cx)))
                    }),
            ],
            cx.listener(|this, _, _, cx| this.onboard_skip(cx)),
        ))
    }
}
