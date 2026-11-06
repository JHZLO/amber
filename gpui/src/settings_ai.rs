// 설정 › AI 연결 — 지금 앱 SettingsModal.tsx 의 AI 칸. 설치된 CLI 를 감지해 카드로(고르면 그 CLI 로), 고른 CLI 의 로그인 상태,
// 경로 직접 지정(접어 둔다 — 감지가 찾으면 영영 안 연다), 모델(큐레이션 / Codex 카탈로그 / 직접 입력), 응답 언어.
// 이 칸만 바닥의 [저장]으로 저장한다 — 닫을 때 저장 안 한 변경이 있으면 묻는다(닫기는 아무것도 잃지 않아야 한다).

use std::cell::Cell;
use std::rc::Rc;

use amber_gpui::ai_config::{self, AppConfig};
use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::menu::{MenuItem, select_menu};
use crate::note_ai_view::{Note, field_label, hint, note_box};
use crate::report_probe::{Auth, Cli, ModelOption};
use crate::settings_view::{glass, set_section};
use crate::theme::Tokens;

/// 고르기 판에서 '직접 입력…' 을 고른 상태 — 모델 id 로 저장되지 않는다
pub const CUSTOM_MODEL: &str = "__custom__";
const DESC_MAX: usize = 56;

pub fn provider_label(p: &str) -> &'static str {
    match p {
        "codex" => "OpenAI Codex CLI",
        _ => "Claude Code",
    }
}

/// 큐레이션 모델(config.ts PROVIDER_MODELS). 빈 id = CLI 기본 모델
pub fn provider_models(p: &str) -> Vec<(String, String)> {
    let m = |id: &str, label: String| (id.to_string(), label);
    if p == "codex" {
        vec![
            m(
                "gpt-6-astra",
                format!("GPT-6-Astra ({})", t("settings.model.latest")),
            ),
            m("gpt-5.6-sol", "GPT-5.6-Sol".into()),
            m("gpt-5.5", "GPT-5.5".into()),
            m("", t("settings.model.cliDefault")),
        ]
    } else {
        vec![
            m(
                "claude-fable-5-1",
                format!("Fable 5.1 ({})", t("settings.model.quality")),
            ),
            m(
                "claude-opus-5-5",
                format!("Opus 5.5 ({})", t("settings.model.latest")),
            ),
            m(
                "claude-opus-5",
                format!("Opus 5 ({})", t("settings.model.previousOpus")),
            ),
            m(
                "claude-sonnet-5",
                format!("Sonnet 5 ({})", t("settings.model.balanced")),
            ),
            m(
                "claude-haiku-4-5-20251001",
                format!("Haiku 4.5 ({})", t("settings.model.fast")),
            ),
            m("", t("settings.model.cliDefault")),
        ]
    }
}

/// mergeModelOptions — 카탈로그가 있으면 그것(+ CLI 기본), 없으면 큐레이션 그대로
pub fn merge_model_options(
    curated: Vec<(String, String)>,
    dynamic: Option<&[ModelOption]>,
    cli_default: &str,
) -> Vec<(String, String)> {
    let Some(list) = dynamic.filter(|d| !d.is_empty()) else {
        return curated;
    };
    let mut out: Vec<(String, String)> = list
        .iter()
        .map(|m| {
            let label = if m.description.is_empty() {
                m.label.clone()
            } else {
                format!("{} \u{2014} {}", m.label, shorten(&m.description, DESC_MAX))
            };
            (m.id.clone(), label)
        })
        .collect();
    out.push((String::new(), cli_default.to_string()));
    out
}

/// 앞뒤 공백을 걷고 max(UTF-16)를 넘으면 max-1 에서 자르고 말줄임
fn shorten(s: &str, max: usize) -> String {
    let t = amber_gpui::vault::js_trim(s);
    let units: Vec<u16> = t.encode_utf16().collect();
    if units.len() > max {
        format!("{}\u{2026}", String::from_utf16_lossy(&units[..max - 1]))
    } else {
        t.to_string()
    }
}

/// 버전 꼬리가 제 이름을 되풀이하면 뗀다("2.1.263 (Claude Code)" → "2.1.263"). 다른 말이 든 괄호는 남긴다
pub fn trim_version(version: &str, name: &str) -> String {
    let v = version.trim();
    if let Some(open) = v.rfind('(')
        && v.ends_with(')')
    {
        let inner = &v[open + 1..v.len() - 1];
        if !inner.contains(['(', ')']) && inner.trim().to_lowercase() == name.trim().to_lowercase()
        {
            return v[..open].trim().to_string();
        }
    }
    v.to_string()
}

/// 저장 대상 넷의 지문 — 경로는 다듬어 비교한다(저장이 다듬어 넣는다)
pub fn ai_snapshot(provider: Option<&str>, path: &str, model: &str, lang: &str) -> String {
    serde_json::json!([provider, amber_gpui::vault::js_trim(path), model, lang]).to_string()
}

pub struct AiTab {
    pub provider: Option<String>,
    pub path: Entity<InputState>,
    pub model: String,
    pub custom: bool,
    pub custom_input: Entity<InputState>,
    pub ai_lang: String,
    /// 연 때(또는 마지막 저장) 값의 지문
    pub saved: String,
    /// 감지 전 None, 빈 목록 = 못 찾음
    pub detected: Option<Vec<Cli>>,
    pub detecting: bool,
    pub codex: Option<Vec<ModelOption>>,
    /// None = 확인 중
    pub auth: Option<Auth>,
    auth_seq: u64,
    pub path_open: bool,
    pub testing: bool,
    pub test_result: Option<(bool, String)>,
    model_open: bool,
    model_anchor: Rc<Cell<Bounds<Pixels>>>,
    lang_open: bool,
    lang_anchor: Rc<Cell<Bounds<Pixels>>>,
    /// 저장 안 한 변경을 안고 닫으려 했다
    pub confirm_close: bool,
    /// 로그인 창(D7f)
    pub auth_open: bool,
}

impl AiTab {
    pub fn model_options(&self, provider: &str) -> Vec<(String, String)> {
        merge_model_options(
            provider_models(provider),
            if provider == "codex" {
                self.codex.as_deref()
            } else {
                None
            },
            &t("settings.model.cliDefault"),
        )
    }

    #[cfg(test)]
    pub fn model_open_for_test(&self) -> bool {
        self.model_open
    }

    pub fn dirty(&self, cx: &App) -> bool {
        let path = self.path.read(cx).value().to_string();
        ai_snapshot(self.provider.as_deref(), &path, &self.model, &self.ai_lang) != self.saved
    }
}

fn wait<T: 'static>(
    rx: futures::channel::oneshot::Receiver<T>,
    cx: &mut AsyncApp,
) -> impl std::future::Future<Output = Option<T>> + use<T> {
    crate::ai::wait(rx, cx.background_executor().clone())
}

impl Spike {
    /// 설정을 열 때 — 저장된 값을 읽고 CLI 를 감지한다(열자마자 카드로 보인다)
    pub(crate) fn mount_ai_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = ai_config::load_config(&self.db).unwrap_or(AppConfig {
            provider: None,
            onboarded: false,
            cli_path: String::new(),
            model: String::new(),
            ai_lang: "auto".into(),
        });
        let path = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(format!(
                "/opt/homebrew/bin/{}",
                c.provider.as_deref().unwrap_or("claude")
            ));
            s.set_value(c.cli_path.clone(), window, cx);
            s
        });
        cx.observe(&path, |_, _, cx| cx.notify()).detach();
        let custom_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(t("settings.model.customPh"));
            s.set_value(c.model.clone(), window, cx);
            s
        });
        cx.subscribe_in(&custom_input, window, |this, s, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let v = amber_gpui::vault::js_trim(&s.read(cx).value()).to_string();
                if let Some(a) = this.settings.ai.as_mut() {
                    a.model = v;
                }
                cx.notify();
            }
        })
        .detach();
        self.settings.ai = Some(AiTab {
            saved: ai_snapshot(c.provider.as_deref(), &c.cli_path, &c.model, &c.ai_lang),
            provider: c.provider.clone(),
            path,
            model: c.model.clone(),
            custom: false,
            custom_input,
            ai_lang: c.ai_lang.clone(),
            detected: None,
            detecting: false,
            codex: None,
            auth: None,
            auth_seq: 0,
            path_open: false,
            testing: false,
            test_result: None,
            model_open: false,
            model_anchor: Rc::default(),
            lang_open: false,
            lang_anchor: Rc::default(),
            confirm_close: false,
            auth_open: false,
        });
        let codex = self.settings.probe.codex_models();
        cx.spawn(async move |this, cx| {
            let list = wait(codex, cx).await.unwrap_or_default();
            let _ = this.update(cx, |s, cx| {
                if let Some(a) = s.settings.ai.as_mut() {
                    a.codex = (!list.is_empty()).then_some(list);
                }
                cx.notify();
            });
        })
        .detach();
        self.ai_redetect(cx);
        if c.provider.is_some() {
            self.ai_refresh_auth(cx);
        }
    }

    fn ai_redetect(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.settings.ai.as_mut() else {
            return;
        };
        a.detecting = true;
        let rx = self.settings.probe.ai_clis();
        cx.spawn(async move |this, cx| {
            let found = wait(rx, cx).await.unwrap_or_default();
            let _ = this.update(cx, |s, cx| {
                if let Some(a) = s.settings.ai.as_mut() {
                    // 아무것도 못 찾았으면 경로 칸을 연다 — 그때는 그게 유일하게 할 수 있는 일이다
                    if found.is_empty() {
                        a.path_open = true;
                    }
                    a.detected = Some(found);
                    a.detecting = false;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 로그인 상태 — 열 때, 프로바이더를 바꿀 때 한 번(경로를 칠 때마다 프로세스를 띄우지 않는다)
    pub(crate) fn ai_refresh_auth(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.settings.ai.as_mut() else {
            return;
        };
        let Some(p) = a.provider.clone() else {
            return;
        };
        a.auth = None;
        a.auth_seq += 1;
        let seq = a.auth_seq;
        let path = Some(a.path.read(cx).value().to_string()).filter(|p| !p.is_empty());
        let rx = self.settings.probe.auth_status(Some(p), path);
        cx.spawn(async move |this, cx| {
            let st = wait(rx, cx).await.unwrap_or(Auth {
                supported: true,
                logged_in: None,
            });
            let _ = this.update(cx, |s, cx| {
                if let Some(a) = s.settings.ai.as_mut()
                    && a.auth_seq == seq
                {
                    a.auth = Some(st);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn ai_pick(&mut self, d: Cli, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.settings.ai.as_mut() else {
            return;
        };
        let first = a
            .model_options(&d.id)
            .first()
            .map(|m| m.0.clone())
            .unwrap_or_default();
        a.provider = Some(d.id.clone());
        a.path
            .update(cx, |st, cx| st.set_value(d.path.clone(), window, cx));
        a.model = first;
        a.custom = false;
        a.test_result = None;
        self.ai_refresh_auth(cx);
    }

    fn ai_test(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.settings.ai.as_mut() else {
            return;
        };
        a.testing = true;
        a.test_result = None;
        let path = Some(a.path.read(cx).value().to_string()).filter(|p| !p.is_empty());
        let rx = self.settings.probe.health(path);
        cx.spawn(async move |this, cx| {
            let r = wait(rx, cx).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(a) = s.settings.ai.as_mut() {
                    a.testing = false;
                    a.test_result = r.map(|r| match r {
                        Ok(v) => (true, t_with("settings.ai.testOk", &[("version", &v)])),
                        Err(e) => (false, e),
                    });
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 바닥 [저장] — 응답 언어는 연결과 무관하게, 연결이 있으면 그 CLI 로 잇는다
    pub(crate) fn ai_save(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.settings.ai.as_ref() else {
            self.close_settings(cx);
            return;
        };
        let path = amber_gpui::vault::js_trim(&a.path.read(cx).value()).to_string();
        let c = AppConfig {
            provider: a.provider.clone(),
            onboarded: true,
            cli_path: path.clone(),
            model: a.model.clone(),
            ai_lang: a.ai_lang.clone(),
        };
        let _ = ai_config::save_config(&self.db, &c);
        if let Some(p) = c.provider.as_deref() {
            let _ = ai_config::connect_provider(&self.db, p, &path, Some(&c.model));
        }
        self.close_settings(cx);
    }

    /// 닫기 — 저장 안 한 변경이 있으면 묻는다
    pub(crate) fn request_close_settings(&mut self, cx: &mut Context<Self>) {
        let dirty = self.settings.ai.as_ref().is_some_and(|a| a.dirty(cx));
        if dirty {
            if let Some(a) = self.settings.ai.as_mut() {
                a.confirm_close = true;
            }
            cx.notify();
        } else {
            self.close_settings(cx);
        }
    }

    /// 열린 고르기 판, 확인 창부터 닫는다. 닫은 게 있으면 true
    pub(crate) fn ai_escape(&mut self) -> bool {
        let Some(a) = self.settings.ai.as_mut() else {
            return false;
        };
        if a.confirm_close {
            a.confirm_close = false;
            return true;
        }
        if a.model_open || a.lang_open {
            a.model_open = false;
            a.lang_open = false;
            return true;
        }
        false
    }

    pub(crate) fn settings_ai_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let Some(a) = self.settings.ai.as_ref() else {
            return div();
        };
        let reduce = cx.reduce_motion();
        let desc = match a.provider.as_deref() {
            Some(p) => t_with("settings.ai.connected", &[("name", provider_label(p))]),
            None => t("settings.ai.none"),
        };
        let detect = button("set-ai-redetect", k, false, a.detecting)
            .child(icon("refresh", 13.).text_color(k.text_2))
            .child(t(if a.detecting {
                "settings.ai.detecting"
            } else {
                "settings.ai.redetect"
            }))
            .when(!a.detecting, |b| {
                b.on_click(cx.listener(|this, _, _, cx| this.ai_redetect(cx)))
            });
        let mut sec = set_section(
            k,
            t("settings.ai.title"),
            Some(desc),
            Some(detect.into_any_element()),
        )
        .tag("set-ai");
        sec = match &a.detected {
            None if a.detecting => sec.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(22.))
                    .child(crate::spinner::spinner(k, reduce))
                    .child(hint(t("settings.ai.searching"), k).mt_0()),
            ),
            Some(list) if list.is_empty() => {
                sec.child(note_box(Note::Error, k).child(t("settings.ai.notFound")))
            }
            _ => sec.child(self.ai_cards(a, k, cx)),
        };
        if let Some(p) = a.provider.clone() {
            sec = sec.child(self.ai_auth_row(a, &p, k, cx));
            let open = a.path_open;
            sec = sec.child(
                div()
                    .id("set-ai-more")
                    .tag("set-ai-more")
                    .mt(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .py(px(3.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(k.text_3)
                    .cursor_pointer()
                    .hover(|s| s.text_color(k.text_2))
                    .child(
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
                    .child(t("settings.ai.advanced"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(a) = this.settings.ai.as_mut() {
                            a.path_open = !a.path_open;
                        }
                        cx.notify();
                    })),
            );
            if open {
                let testing = a.testing;
                sec = sec.child(
                    div()
                        .mt(px(10.))
                        .pl(px(14.))
                        .border_l(px(2.))
                        .border_color(k.border)
                        .flex()
                        .flex_col()
                        .child(field_label(
                            t_with("settings.ai.pathLabel", &[("name", provider_label(&p))]),
                            k,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .max_w(px(440.))
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(k.border_strong)
                                        .bg(k.surface)
                                        .px(px(11.))
                                        .py(px(4.))
                                        .text_size(px(14.))
                                        .tag("set-ai-path")
                                        .child(Input::new(&a.path).appearance(false).px_0()),
                                )
                                .child(
                                    button("set-ai-test", k, false, testing)
                                        .when(testing, |b| {
                                            b.child(crate::spinner::spinner(k, reduce))
                                        })
                                        .when(!testing, |b| b.child(t("settings.ai.test")))
                                        .when(!testing, |b| {
                                            b.on_click(
                                                cx.listener(|this, _, _, cx| this.ai_test(cx)),
                                            )
                                        }),
                                ),
                        )
                        .child(hint(t("settings.ai.advancedHint"), k).max_w(px(470.)))
                        .when_some(a.test_result.clone(), |d, (ok, msg)| {
                            d.child(
                                note_box(if ok { Note::Ok } else { Note::Error }, k)
                                    .mt(px(8.))
                                    .tag("set-ai-test-result")
                                    .child(msg),
                            )
                        }),
                );
            }
        }
        let mut out = div().flex().flex_col().child(sec);
        if let Some(p) = a.provider.clone() {
            out = out.child(
                set_section(k, t("settings.ai.behaviorTitle"), None, None)
                    .mt(px(20.))
                    .pt(px(20.))
                    .border_t_1()
                    .border_color(k.border)
                    .child(self.ai_behavior(a, &p, k, cx)),
            );
        }
        out
    }

    fn ai_cards(&self, a: &AiTab, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut grid = div().flex().flex_wrap().gap(px(8.));
        for d in a.detected.clone().unwrap_or_default() {
            let checked = a.provider.as_deref() == Some(d.id.as_str());
            let (hf, he) = (glass(k.glass_fill_hover), k.glass_edge_hover);
            let text_2 = k.text_2;
            let pick = d.clone();
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("set-ai-card-{}", d.id)))
                    .tag(format!("set-ai-card-{}", d.id))
                    .flex_1()
                    .min_w(px(240.))
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
                                    .text_color(if checked { k.text } else { text_2 })
                                    .child(d.name.clone()),
                            )
                            .child(
                                div()
                                    .font_family(crate::markdown::MONO)
                                    .text_size(px(11.5))
                                    .text_color(k.text_3)
                                    .child(trim_version(&d.version, &d.name)),
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ai_pick(pick.clone(), window, cx)
                    })),
            );
        }
        grid
    }

    /// 로그인 상태 — 행동이 필요할 때만 단추를 낸다
    fn ai_auth_row(&self, a: &AiTab, p: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .mt(px(12.))
            .tag("set-ai-auth");
        let state = |text: String| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.5))
                .text_color(k.text_3)
                .child(text)
        };
        let open_login = || {
            cx.listener(|this, _, _, cx| {
                if let Some(a) = this.settings.ai.as_mut() {
                    a.auth_open = true;
                }
                cx.notify();
            })
        };
        match &a.auth {
            None => row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(k.text_3)
                    .child(crate::spinner::spinner(k, cx.reduce_motion()))
                    .child(t("settings.auth.checking")),
            ),
            Some(s) if !s.supported => row.child(state(t_with(
                "settings.auth.unsupported",
                &[("name", provider_label(p))],
            ))),
            Some(s) if s.logged_in == Some(true) => row
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.5))
                        .text_color(k.success_text)
                        .child(icon("check", 13.).text_color(k.success))
                        .child(t("settings.auth.rowOk")),
                )
                .child(
                    button("set-ai-login-again", k, false, false)
                        .child(t("settings.auth.again"))
                        .on_click(open_login()),
                ),
            Some(s) if s.logged_in == Some(false) => row
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(k.text)
                        .child(icon("alert-triangle", 13.).text_color(k.warning))
                        .child(t("settings.auth.rowExpired")),
                )
                .child(
                    primary_button("set-ai-login", k, false)
                        .child(t("settings.auth.rowAction"))
                        .on_click(open_login()),
                ),
            Some(_) => row.child(state(t("settings.auth.rowUnknown"))),
        }
    }

    fn ai_behavior(&self, a: &AiTab, p: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let options = a.model_options(p);
        let known = options.iter().any(|(id, _)| *id == a.model);
        let show_custom = a.custom || !known;
        let model_label = if show_custom {
            t("settings.model.custom")
        } else {
            options
                .iter()
                .find(|(id, _)| *id == a.model)
                .map(|(_, l)| l.clone())
                .unwrap_or_default()
        };
        let lang_label = match a.ai_lang.as_str() {
            "ko" => "한국어".to_string(),
            "en" => "English".to_string(),
            _ => t("settings.ai.langAuto"),
        };
        let trigger = |id: &'static str, label: String, anchor: Rc<Cell<Bounds<Pixels>>>| {
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
        };
        let col = |d: Div| d.flex_1().min_w(px(260.)).flex().flex_col();
        div()
            .flex()
            .flex_wrap()
            .gap_x(px(18.))
            .child(
                col(div())
                    .child(field_label(t("settings.ai.modelLabel"), k))
                    .child(
                        trigger("set-ai-model", model_label, a.model_anchor.clone()).on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some(a) = this.settings.ai.as_mut() {
                                    a.model_open = !a.model_open;
                                }
                                cx.notify();
                            }),
                        ),
                    )
                    .when(show_custom, |d| {
                        d.child(
                            div()
                                .mt(px(8.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.border_strong)
                                .bg(k.surface)
                                .px(px(11.))
                                .py(px(4.))
                                .font_family(crate::markdown::MONO)
                                .text_size(px(13.))
                                .tag("set-ai-model-custom")
                                .child(Input::new(&a.custom_input).appearance(false).px_0()),
                        )
                    })
                    .child(hint(
                        t(if show_custom {
                            "settings.model.customHint"
                        } else {
                            "settings.ai.creditHint"
                        }),
                        k,
                    )),
            )
            .child(
                col(div())
                    .child(field_label(t("settings.ai.langLabel"), k))
                    .child(
                        trigger("set-ai-lang", lang_label, a.lang_anchor.clone()).on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some(a) = this.settings.ai.as_mut() {
                                    a.lang_open = !a.lang_open;
                                }
                                cx.notify();
                            }),
                        ),
                    )
                    .child(hint(t("settings.ai.langHint"), k)),
            )
    }

    /// 모델, 언어 고르기 판(창 좌표라 뿌리에 그린다)
    pub(crate) fn settings_ai_menus(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Deferred> {
        let a = self.settings.ai.as_ref()?;
        if !self.settings.open {
            return None;
        }
        let p = a.provider.clone()?;
        if a.model_open {
            let options = a.model_options(&p);
            let known = options.iter().any(|(id, _)| *id == a.model);
            let show_custom = a.custom || !known;
            let mut values: Vec<(String, String)> = options;
            values.push((CUSTOM_MODEL.into(), t("settings.model.custom")));
            let cur = if show_custom {
                CUSTOM_MODEL.to_string()
            } else {
                a.model.clone()
            };
            let items = values
                .iter()
                .map(|(v, l)| MenuItem {
                    label: l.clone(),
                    checked: *v == cur,
                    ..Default::default()
                })
                .collect();
            let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
            return Some(select_menu(
                "set-ai-model-menu",
                a.model_anchor.get(),
                items,
                k,
                move |i, _, cx| {
                    let Some((v, _)) = values.get(i).cloned() else {
                        return;
                    };
                    let _ = pick.update(cx, |this, cx| {
                        if let Some(a) = this.settings.ai.as_mut() {
                            a.model_open = false;
                            if v == CUSTOM_MODEL {
                                let known =
                                    a.model_options(&p).iter().any(|(id, _)| *id == a.model);
                                a.custom = true;
                                if known {
                                    a.model = String::new();
                                }
                            } else {
                                a.custom = false;
                                a.model = v;
                            }
                        }
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |this, cx| {
                        if let Some(a) = this.settings.ai.as_mut() {
                            a.model_open = false;
                        }
                        cx.notify();
                    });
                },
            ));
        }
        if a.lang_open {
            let values = vec![
                ("auto".to_string(), t("settings.ai.langAuto")),
                ("ko".to_string(), "한국어".to_string()),
                ("en".to_string(), "English".to_string()),
            ];
            let items = values
                .iter()
                .map(|(v, l)| MenuItem {
                    label: l.clone(),
                    checked: *v == a.ai_lang,
                    ..Default::default()
                })
                .collect();
            let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
            return Some(select_menu(
                "set-ai-lang-menu",
                a.lang_anchor.get(),
                items,
                k,
                move |i, _, cx| {
                    let Some((v, _)) = values.get(i).cloned() else {
                        return;
                    };
                    let _ = pick.update(cx, |this, cx| {
                        if let Some(a) = this.settings.ai.as_mut() {
                            a.lang_open = false;
                            a.ai_lang = v;
                        }
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |this, cx| {
                        if let Some(a) = this.settings.ai.as_mut() {
                            a.lang_open = false;
                        }
                        cx.notify();
                    });
                },
            ));
        }
        None
    }

    /// 저장 안 한 변경 확인 — 버리는 쪽만 빨강
    pub(crate) fn settings_unsaved_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if !self.settings.open || !self.settings.ai.as_ref().is_some_and(|a| a.confirm_close) {
            return None;
        }
        let keep = || {
            cx.listener(|this, _, _, cx| {
                if let Some(a) = this.settings.ai.as_mut() {
                    a.confirm_close = false;
                }
                cx.notify();
            })
        };
        Some(crate::dialog::shell(
            k,
            440.,
            t("common.unsaved.title"),
            div().child(t("common.unsaved.body")).into_any_element(),
            vec![
                button("set-unsaved-keep", k, false, false)
                    .child(t("common.unsaved.keep"))
                    .on_click(keep()),
                crate::day_card::danger_button("set-unsaved-discard", k)
                    .child(t("common.unsaved.discard"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_settings(cx))),
            ],
            keep(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ai_snapshot, merge_model_options, trim_version};
    use crate::report_probe::ModelOption;

    // settingsModal.test.ts 의 갈래
    #[test]
    fn trim_version_drops_only_a_repeated_name() {
        assert_eq!(
            trim_version("2.1.263 (Claude Code)", "Claude Code"),
            "2.1.263"
        );
        assert_eq!(
            trim_version("2.1.263 (claude code)", "Claude Code"),
            "2.1.263"
        );
        assert_eq!(
            trim_version("0.153.4 (rust build)", "OpenAI Codex CLI"),
            "0.153.4 (rust build)"
        );
        assert_eq!(
            trim_version("codex-cli 0.153.4", "OpenAI Codex CLI"),
            "codex-cli 0.153.4"
        );
        assert_eq!(trim_version("  2.1.263  ", "Claude Code"), "2.1.263");
    }

    #[test]
    fn snapshot_ignores_path_spaces_but_not_other_changes() {
        let base = ai_snapshot(Some("claude"), "/bin/claude", "opus", "ko");
        assert_eq!(
            ai_snapshot(Some("claude"), "  /bin/claude  ", "opus", "ko"),
            base
        );
        assert_ne!(
            ai_snapshot(Some("codex"), "/bin/claude", "opus", "ko"),
            base
        );
        assert_ne!(
            ai_snapshot(Some("claude"), "/bin/claude", "sonnet", "ko"),
            base
        );
        assert_ne!(
            ai_snapshot(Some("claude"), "/bin/claude", "opus", "en"),
            base
        );
        assert_ne!(
            ai_snapshot(None, "", "", "auto"),
            ai_snapshot(Some("claude"), "", "", "auto")
        );
        assert_eq!(
            base, r#"["claude","/bin/claude","opus","ko"]"#,
            "JSON.stringify 와 같다"
        );
    }

    // modelOptions.test.ts 의 갈래
    #[test]
    fn model_options_use_the_catalog_when_there_is_one() {
        let curated = vec![
            ("a".into(), "A".into()),
            (String::new(), "CLI default".into()),
        ];
        assert_eq!(
            merge_model_options(curated.clone(), None, "CLI default"),
            curated
        );
        assert_eq!(
            merge_model_options(curated.clone(), Some(&[]), "CLI default"),
            curated
        );
        let out = merge_model_options(
            curated,
            Some(&[
                ModelOption {
                    id: "gpt-x".into(),
                    label: "GPT-X".into(),
                    description: "x".repeat(80),
                },
                ModelOption {
                    id: "gpt-y".into(),
                    label: "GPT-Y".into(),
                    description: String::new(),
                },
            ]),
            "CLI default",
        );
        let ids: Vec<&str> = out.iter().map(|o| o.0.as_str()).collect();
        assert_eq!(ids, ["gpt-x", "gpt-y", ""]);
        assert!(out[0].1.starts_with("GPT-X \u{2014} ") && out[0].1.ends_with('\u{2026}'));
        assert_eq!(
            out[0].1.chars().count(),
            "GPT-X \u{2014} ".chars().count() + 56
        );
        assert_eq!(out[1].1, "GPT-Y");
        assert_eq!(out[2].1, "CLI default");
    }
}
