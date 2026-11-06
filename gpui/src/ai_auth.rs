// AI 로그인 창 — 지금 앱 AiAuthModal.tsx. 설정의 로그인 단추에서도, 어느 AI 기능이 AI_AUTH 로 실패했을 때도 같은 창이 뜬다.
// 확인 중 → (로그인됨: '이미 로그인' | 아니면 안내) → [로그인 시작] → CLI 가 흘리는 글에서 인증 주소를 뽑아 [브라우저에서 열기],
// 코드를 물으면 입력칸 → [코드 전달] → 끝나면 '로그인됐어요'. 코드는 저장하지 않고 CLI 에 그대로 넘긴다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::day_card::{Tag, button, primary_button};
use crate::note_ai_view::{Note, field_label, hint, note_box};
use crate::report_probe::{Auth, LoginBox};
use crate::settings_ai::provider_label;
use crate::theme::Tokens;

/// 흘러온 글에서 인증 주소 하나 — 끝의 구두점은 뺀다
pub fn extract_auth_url(output: &str) -> Option<String> {
    let start = output.find("https://")?;
    let rest = &output[start..];
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches(['.', ',', ')', '"', '\'']);
    Some(url.to_string())
}

/// CLI 가 코드를 붙여넣으라고 물었나 — 주소 안의 code=… 에 걸리지 않게 꼬리 200자(UTF-16)만 본다
pub fn asks_for_code(output: &str) -> bool {
    let units: Vec<u16> = output.encode_utf16().collect();
    let tail = String::from_utf16_lossy(&units[units.len().saturating_sub(200)..]);
    tail.to_lowercase().contains("paste")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Checking,
    Idle,
    Running,
    Done,
}

pub struct AuthModal {
    pub provider: Option<String>,
    pub cli_path: Option<String>,
    pub phase: Phase,
    pub status: Option<Auth>,
    pub output: String,
    pub code: Entity<InputState>,
    pub busy: bool,
    pub err: Option<String>,
    sink: Arc<Mutex<LoginBox>>,
    seq: u64,
}

impl Spike {
    /// 연다 — 지금 연결(또는 설정 칸에서 고른 것)의 로그인 상태부터 본다
    pub(crate) fn open_auth(
        &mut self,
        provider: Option<String>,
        cli_path: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let code = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("settings.auth.codePlaceholder")));
        cx.subscribe_in(&code, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.auth_submit(cx),
            _ => {}
        })
        .detach();
        let rx = self
            .settings
            .probe
            .auth_status(provider.clone(), cli_path.clone());
        self.auth = Some(AuthModal {
            provider,
            cli_path,
            phase: Phase::Checking,
            status: None,
            output: String::new(),
            code,
            busy: false,
            err: None,
            sink: Arc::default(),
            seq: 0,
        });
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let st = crate::ai::wait(rx, ex).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(m) = s.auth.as_mut() {
                    m.status = st;
                    m.phase = Phase::Idle;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 닫기 — 진행 중인 로그인은 끊는다
    pub(crate) fn close_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.auth.take().is_some() {
            self.settings.probe.auth_cancel();
        }
        self.focus.focus(window, cx);
        // 설정 칸이 열려 있으면 상태를 다시 본다
        self.ai_refresh_auth(cx);
        cx.notify();
    }

    fn auth_start(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.auth.as_mut() else {
            return;
        };
        m.output.clear();
        m.err = None;
        m.phase = Phase::Running;
        m.seq += 1;
        let seq = m.seq;
        m.sink = Arc::default();
        let sink = m.sink.clone();
        let rx = self
            .settings
            .probe
            .login(m.provider.clone(), m.cli_path.clone(), sink.clone());
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let started = crate::ai::wait(rx, ex.clone()).await;
            if let Some(Err(e)) = started {
                let _ = this.update(cx, |s, cx| {
                    if let Some(m) = s.auth.as_mut().filter(|m| m.seq == seq) {
                        m.err = Some(e);
                        m.phase = Phase::Idle;
                    }
                    cx.notify();
                });
                return;
            }
            // 글과 끝을 100ms 마다 옮긴다
            loop {
                let done = this.update(cx, |s, cx| {
                    let Some(m) = s.auth.as_mut().filter(|m| m.seq == seq) else {
                        return true;
                    };
                    let Ok(mut b) = sink.lock() else {
                        return true;
                    };
                    if b.changed {
                        b.changed = false;
                        m.output.clone_from(&b.output);
                        cx.notify();
                    }
                    if let Some(st) = b.done.take() {
                        let ok = st.logged_in == Some(true);
                        m.status = Some(st);
                        m.phase = Phase::Done;
                        cx.notify();
                        drop(b);
                        if ok {
                            s.ai_refresh_auth(cx);
                        }
                        return true;
                    }
                    false
                });
                if !matches!(done, Ok(false)) {
                    break;
                }
                ex.timer(Duration::from_millis(100)).await;
            }
        })
        .detach();
        cx.notify();
    }

    fn auth_submit(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.auth.as_mut() else {
            return;
        };
        let code = m.code.read(cx).value().to_string();
        if amber_gpui::vault::js_trim(&code).is_empty() || m.busy {
            return;
        }
        m.busy = true;
        m.err = None;
        let rx = self.settings.probe.auth_code(code);
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let r = crate::ai::wait(rx, ex).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(m) = s.auth.as_mut() {
                    m.busy = false;
                    if let Some(Err(e)) = r {
                        m.err = Some(e);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// AI 호출이 AI_AUTH 로 실패했다 — 지금 연결로 로그인 창을 연다(그리기 전 단계에서)
    pub(crate) fn take_auth_required(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::ai::take_auth_required() || self.auth.is_some() {
            return;
        }
        let c = amber_gpui::ai_config::load_config(&self.db).ok();
        let provider = c.as_ref().and_then(|c| c.provider.clone());
        let path = c.map(|c| c.cli_path).filter(|p| !p.is_empty());
        self.open_auth(provider, path, window, cx);
    }

    pub(crate) fn auth_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let m = self.auth.as_ref()?;
        let name = m
            .provider
            .as_deref()
            .map(provider_label)
            .unwrap_or_default();
        let logged_in = m.status.as_ref().is_some_and(|s| s.logged_in == Some(true));
        let unsupported = m.status.as_ref().is_some_and(|s| !s.supported);
        let url = extract_auth_url(&m.output);
        let wants_code = asks_for_code(&m.output);
        let last = m
            .output
            .trim()
            .lines()
            .filter(|l| !l.is_empty())
            .next_back()
            .unwrap_or_default()
            .to_string();
        let desc = |text: String| {
            div()
                .mb(px(12.))
                .text_size(px(12.5))
                .line_height(relative(1.6))
                .text_color(k.text_3)
                .child(text)
        };
        let mut body = div().flex().flex_col();
        if m.phase == Phase::Checking {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(22.))
                    .text_color(k.text_2)
                    .child(crate::spinner::spinner(k, cx.reduce_motion()))
                    .child(t("settings.auth.checking")),
            );
        } else if unsupported {
            body = body.child(desc(t_with("settings.auth.unsupported", &[("name", name)])));
        } else {
            body = body.child(desc(if m.phase == Phase::Done && logged_in {
                t("settings.auth.done")
            } else if logged_in && m.phase == Phase::Idle {
                t_with("settings.auth.alreadyOk", &[("name", name)])
            } else {
                t_with("settings.auth.lead", &[("name", name)])
            }));
            if let Some(url) = url.clone().filter(|_| m.phase != Phase::Done) {
                let open = url.clone();
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .mb(px(16.))
                        .child(field_label(t("settings.auth.urlLabel"), k))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(k.border_strong)
                                        .bg(k.surface)
                                        .px(px(11.))
                                        .py(px(7.))
                                        .text_size(px(13.))
                                        .tag("auth-url")
                                        .child(url),
                                )
                                .child(
                                    button("auth-open", k, false, false)
                                        .child(t("settings.auth.openBrowser"))
                                        .on_click(move |_, _, cx| cx.open_url(&open)),
                                ),
                        )
                        .child(hint(t("settings.auth.urlHint"), k)),
                );
            }
            if m.phase == Phase::Running && wants_code {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .child(field_label(t("settings.auth.codeLabel"), k))
                        .child(
                            div()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.border_strong)
                                .bg(k.surface)
                                .px(px(11.))
                                .py(px(4.))
                                .text_size(px(14.))
                                .tag("auth-code")
                                .child(Input::new(&m.code).appearance(false).px_0()),
                        )
                        .child(hint(t("settings.auth.codeSafety"), k)),
                );
            }
            if m.phase == Phase::Running && !wants_code && url.is_none() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.5))
                        .text_color(k.text_3)
                        .child(crate::spinner::spinner(k, cx.reduce_motion()))
                        .child(t("settings.auth.waiting")),
                );
            }
            if m.phase == Phase::Done && !logged_in {
                body = body.child(
                    note_box(Note::Error, k)
                        .child(t("settings.auth.failed"))
                        .when(!last.is_empty(), |d| {
                            d.child(div().mt(px(4.)).text_size(px(12.)).child(last.clone()))
                        }),
                );
            }
        }
        if let Some(e) = m.err.clone() {
            body = body.child(note_box(Note::Error, k).mt(px(12.)).child(e));
        }
        let mut right = Vec::new();
        if !unsupported && !(m.phase == Phase::Done && logged_in) {
            if wants_code && m.phase == Phase::Running {
                let empty = amber_gpui::vault::js_trim(&m.code.read(cx).value()).is_empty();
                let off = m.busy || empty;
                right.push(
                    primary_button("auth-submit", k, off)
                        .child(t("settings.auth.submit"))
                        .when(!off, |b| {
                            b.on_click(cx.listener(|this, _, _, cx| this.auth_submit(cx)))
                        }),
                );
            } else {
                let off = matches!(m.phase, Phase::Checking | Phase::Running);
                right.push(
                    primary_button("auth-start", k, off)
                        .child(t(if m.phase == Phase::Idle && !logged_in {
                            "settings.auth.start"
                        } else {
                            "settings.auth.again"
                        }))
                        .when(!off, |b| {
                            b.on_click(cx.listener(|this, _, _, cx| this.auth_start(cx)))
                        }),
                );
            }
        }
        Some(crate::dialog::shell_split(
            k,
            440.,
            t("settings.auth.title"),
            body.into_any_element(),
            vec![
                button("auth-close", k, false, false)
                    .child(t(if m.phase == Phase::Done && logged_in {
                        "common.done"
                    } else {
                        "common.close"
                    }))
                    .on_click(cx.listener(|this, _, window, cx| this.close_auth(window, cx))),
            ],
            right,
            cx.listener(|this, _, window, cx| this.close_auth(window, cx)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{asks_for_code, extract_auth_url};

    // auth.test.ts 의 표본(claude auth login --claudeai 의 실제 stdout)
    const URL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile&code_challenge=7yrPyvKDb8Tl0AsapOGymXjbwWfuWoh8&code_challenge_method=S256&state=q-ZiEPvcCD5C6wccD-Ih7iTcNKAfoQ_9Mq3ADWHKT0s";

    fn stream() -> String {
        format!(
            "Opening browser to sign in\u{2026}\nIf the browser didn't open, visit: {URL}\nPaste code here if prompted > "
        )
    }

    #[test]
    fn the_auth_url_is_pulled_out_whole() {
        assert_eq!(extract_auth_url(&stream()).as_deref(), Some(URL));
        assert_eq!(
            extract_auth_url("visit: https://example.com/a.").as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(extract_auth_url("Opening browser to sign in\u{2026}"), None);
    }

    #[test]
    fn only_the_prompt_tail_asks_for_a_code() {
        assert!(asks_for_code(&stream()));
        assert!(!asks_for_code(&format!(
            "If the browser didn't open, visit: {URL}"
        )));
        assert!(!asks_for_code(
            "Starting local login server on http://localhost:1455"
        ));
    }
}
