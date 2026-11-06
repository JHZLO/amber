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
    /// AI 칸 — 열 때 만든다(바닥 [저장]이 이 칸의 값을 쓴다)
    pub ai: Option<crate::settings_ai::AiTab>,
    /// gh, claude 확인 — 시험 빌드는 늘 가짜(사용자의 gh, claude 에 닿지 않게)
    pub probe: Rc<dyn ReportProbe>,
    /// 오른쪽 판의 스크롤 — 칸을 바꾸면 맨 위로
    pub pane: ScrollHandle,
    /// 모양 칸 — 테마/언어 고르기 판, 확인을 기다리는 언어
    pub theme_open: bool,
    pub lang_open: bool,
    pub lang_pending: Option<&'static str>,
    /// 프롬프트 칸 — 집중 편집기(열려 있으면 설정 판 대신 뜬다)와 지우기 확인(id, 이름)
    pub prompt_edit: Option<crate::settings_prompts::PromptEdit>,
    pub prompt_del: Option<(String, String)>,
    /// 데이터베이스 칸 — 지우기 확인, 비밀번호 다시 넣기, 오류
    pub db_delete: Option<amber_gpui::dbconn::DbConnection>,
    pub db_password: Option<crate::settings_db::DbPassword>,
    pub db_error: Option<String>,
    pub theme_anchor: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub lang_anchor: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
}

impl Default for SettingsUi {
    fn default() -> Self {
        SettingsUi {
            open: false,
            tab: Tab::Ai,
            rep: None,
            ai: None,
            probe: if cfg!(test) {
                Rc::new(Fake::default())
            } else {
                Rc::new(Core)
            },
            pane: ScrollHandle::new(),
            theme_open: false,
            lang_open: false,
            lang_pending: None,
            prompt_edit: None,
            prompt_del: None,
            db_delete: None,
            db_password: None,
            db_error: None,
            theme_anchor: Default::default(),
            lang_anchor: Default::default(),
        }
    }
}

impl Spike {
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.open = true;
        self.settings.tab = Tab::Ai;
        self.settings.rep = None;
        self.settings.ai = None;
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.open = false;
        self.settings.rep = None;
        self.settings.ai = None;
        self.settings.prompt_edit = None;
        self.settings.prompt_del = None;
        self.settings.db_delete = None;
        self.settings.db_password = None;
        self.settings.db_error = None;
        cx.notify();
    }

    /// 그리기 전에 — 보이는 칸의 상태를 만든다(창이 필요하다)
    pub(crate) fn prepare_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.open && self.settings.ai.is_none() {
            self.mount_ai_tab(window, cx);
        }
        if self.settings.open && self.settings.tab == Tab::Report && self.settings.rep.is_none() {
            self.mount_report_settings(window, cx);
        }
        self.sync_report_settings(window, cx);
    }

    pub(crate) fn settings_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if !self.settings.open {
            return None;
        }
        if let Some(e) = self.settings.prompt_edit.as_ref() {
            return Some(self.prompt_editor(e, k, cx));
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
            .when(cur == Tab::Ai, |d| d.child(self.settings_ai_tab(k, cx)))
            .when(cur == Tab::Prompts, |d| d.child(self.prompts_tab(k, cx)))
            .when(cur == Tab::Databases, |d| {
                d.child(self.databases_tab(k, cx))
            })
            .when(cur == Tab::Report, |d| d.child(self.report_settings(k, cx)))
            .when(cur == Tab::Appearance, |d| {
                d.child(self.appearance_tab(k, cx))
            });
        let body = div().size_full().flex().child(nav).child(pane);
        Some(crate::dialog::shell_settings(
            k,
            t("settings.title"),
            body.into_any_element(),
            vec![
                button("set-close", k, false, false)
                    .child(t("common.close"))
                    .on_click(cx.listener(|this, _, _, cx| this.request_close_settings(cx))),
                {
                    let testing = self.settings.ai.as_ref().is_some_and(|a| a.testing);
                    crate::day_card::primary_button("set-save", k, testing)
                        .child(t("common.save"))
                        .when(!testing, |b| {
                            b.on_click(cx.listener(|this, _, _, cx| this.ai_save(cx)))
                        })
                },
            ],
            cx.listener(|this, _, _, cx| this.request_close_settings(cx)),
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

const THEMES: [(&str, &str); 3] = [
    ("system", "settings.theme.system"),
    ("light", "settings.theme.light"),
    ("dark", "settings.theme.dark"),
];

/// 언어는 각 언어의 자기 표기(번역하지 않는다)
const LANGS: [(&str, &str); 2] = [("ko", "한국어"), ("en", "English")];

/// 고르기 단추 — 유리, 꺾쇠, 자리를 재 두어 판을 그 아래에 띄운다
pub(crate) fn select_trigger(
    id: &'static str,
    label: String,
    anchor: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    k: &Tokens,
) -> Stateful<Div> {
    let (hf, he) = (glass(k.glass_fill_hover), k.glass_edge_hover);
    div()
        .id(id)
        .tag(id)
        .relative()
        .h(px(34.))
        .w_full()
        .pl(px(12.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(k.glass_edge)
        .bg(glass(k.glass_fill))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| s.bg(hf).border_color(he))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label),
        )
        .child(icon("chevron-down", 14.).text_color(k.text_3))
        .child(
            canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
}

impl Spike {
    fn theme_pref(&self) -> &'static str {
        match amber_gpui::prefs::get(&self.db, "til.theme").as_deref() {
            Some("light") => "light",
            Some("dark") => "dark",
            _ => "system",
        }
    }

    /// 모양 — 테마(즉시), 언어(다시 띄워야 적용돼 한 번 묻는다)
    fn appearance_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let theme = self.theme_pref();
        let lang = if amber_gpui::i18n::is_ko() {
            "ko"
        } else {
            "en"
        };
        let field = |label: String, control: Stateful<Div>, h: Option<String>| {
            div()
                .flex()
                .flex_col()
                .mb(px(16.))
                .max_w(px(440.))
                .child(crate::note_ai_view::field_label(label, k))
                .child(control)
                .when_some(h, |d, h| d.child(crate::note_ai_view::hint(h, k)))
        };
        set_section(k, t("settings.tab.appearance"), None, None)
            .tag("set-appearance")
            .child(field(
                t("settings.theme.label"),
                select_trigger(
                    "set-theme",
                    t(THEMES
                        .iter()
                        .find(|(id, _)| *id == theme)
                        .map_or("settings.theme.system", |x| x.1)),
                    self.settings.theme_anchor.clone(),
                    k,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings.theme_open = !this.settings.theme_open;
                    cx.notify();
                })),
                None,
            ))
            .child(field(
                t("settings.lang.label"),
                select_trigger(
                    "set-lang",
                    LANGS
                        .iter()
                        .find(|(id, _)| *id == lang)
                        .map_or("", |x| x.1)
                        .to_string(),
                    self.settings.lang_anchor.clone(),
                    k,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings.lang_open = !this.settings.lang_open;
                    cx.notify();
                })),
                Some(t("settings.lang.hint")),
            ))
    }

    /// 테마 즉시 적용 — 지금 앱의 til.theme 그대로
    pub(crate) fn set_theme_pref(
        &mut self,
        pref: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        amber_gpui::prefs::set(&self.db, "til.theme", pref);
        crate::theme::apply(Some(pref), window.appearance(), cx);
        // 테마를 따라 다시 그려 두는 그림(표식 등)은 다음 그리기에서 다시 굽는다
        cx.notify();
    }

    /// 언어 적용 — 저장하고 앱을 다시 띄운다(지금 앱은 창을 다시 불러온다)
    fn apply_lang(&mut self, cx: &mut Context<Self>) {
        let Some(lang) = self.settings.lang_pending.take() else {
            return;
        };
        amber_gpui::prefs::set(&self.db, "amber.lang", lang);
        if !cfg!(test) {
            cx.restart();
        }
        cx.notify();
    }

    pub(crate) fn appearance_menu(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Deferred> {
        if !self.settings.open || self.settings.tab != Tab::Appearance {
            return None;
        }
        if self.settings.theme_open {
            let cur = self.theme_pref();
            let items = THEMES
                .iter()
                .map(|(id, label)| crate::menu::MenuItem {
                    label: t(label),
                    checked: *id == cur,
                    ..Default::default()
                })
                .collect();
            let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
            return Some(crate::menu::select_menu(
                "set-theme-menu",
                self.settings.theme_anchor.get(),
                items,
                k,
                move |i, window, cx| {
                    let _ = pick.update(cx, |this, cx| {
                        this.settings.theme_open = false;
                        this.set_theme_pref(THEMES[i].0, window, cx);
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.settings.theme_open = false;
                        cx.notify();
                    });
                },
            ));
        }
        if self.settings.lang_open {
            let cur = if amber_gpui::i18n::is_ko() {
                "ko"
            } else {
                "en"
            };
            let items = LANGS
                .iter()
                .map(|(id, label)| crate::menu::MenuItem {
                    label: label.to_string(),
                    checked: *id == cur,
                    ..Default::default()
                })
                .collect();
            let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
            return Some(crate::menu::select_menu(
                "set-lang-menu",
                self.settings.lang_anchor.get(),
                items,
                k,
                move |i, _, cx| {
                    let _ = pick.update(cx, |this, cx| {
                        this.settings.lang_open = false;
                        let v = LANGS[i].0;
                        if v != cur {
                            this.settings.lang_pending = Some(v);
                        }
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.settings.lang_open = false;
                        cx.notify();
                    });
                },
            ));
        }
        None
    }

    /// 언어 바꾸기 확인 — 설정 위에 좁게
    pub(crate) fn lang_confirm_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        self.settings.lang_pending?;
        let cancel = || {
            cx.listener(|this, _, _, cx| {
                this.settings.lang_pending = None;
                cx.notify();
            })
        };
        Some(crate::dialog::shell(
            k,
            440.,
            t("settings.lang.confirmTitle"),
            div()
                .child(t("settings.lang.confirmBody"))
                .into_any_element(),
            vec![
                button("set-lang-cancel", k, false, false)
                    .child(t("common.cancel"))
                    .on_click(cancel()),
                crate::day_card::primary_button("set-lang-apply", k, false)
                    .child(t("settings.lang.apply"))
                    .on_click(cx.listener(|this, _, _, cx| this.apply_lang(cx))),
            ],
            cancel(),
        ))
    }
}
