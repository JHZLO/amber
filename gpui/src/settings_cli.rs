// 설정 › CLI — 터미널에서 Amber 를 다룰 기기를 연결하고 끊는다. [기기 추가]를 누르면 같은 판에 단계가 열린다:
// 1 `amber` 명령 설치(한 번), 2 에이전트나 터미널에서 `amber connect`, 3 요청을 기다리다가 들어오면 그 자리에서 [허용].
// 허용하면 판이 닫히고 새 기기가 목록에서 잠깐 밝게 보인다. 요청은 이 판이 열려 있을 때만 받는다(cli.rs).

use amber_gpui::i18n::{t, t_with};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::cli::{Device, devices, install_path, installed, local_time};
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{Note, note_box};
use crate::settings_view::set_section;
use crate::theme::Tokens;

/// 프로그램 머리글자 — Claude Code, Codex, 그 밖의 터미널
fn avatar(kind: &str, k: &Tokens) -> Div {
    let (label, mono) = match kind {
        "claude" => ("C", false),
        "codex" => ("X", false),
        _ => (">_", true),
    };
    div()
        .size(px(30.))
        .flex_none()
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .bg(k.surface_3)
        .border_1()
        .border_color(k.border)
        .text_color(k.text_2)
        .font_weight(FontWeight::BOLD)
        .text_size(px(if mono { 11.5 } else { 13. }))
        .when(mono, |d| d.font_family(crate::markdown::MONO))
        .child(label)
}

/// 프로그램 이름 — 모르면 '터미널'
fn program_label(program: &str) -> String {
    if program.is_empty() {
        t("settings.cli.terminal")
    } else {
        program.to_string()
    }
}

/// 단계 번호 — 끝난 단계는 체크, 지금 단계는 밝게
fn step_num(n: &'static str, done: bool, now: bool, k: &Tokens) -> Div {
    div()
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .map(|d| {
            if done {
                d.bg(k.success_soft)
                    .border_color(k.success)
                    .text_color(k.success)
                    .child(icon("check", 12.).text_color(k.success))
            } else if now {
                d.border_color(k.text).text_color(k.text).child(n)
            } else {
                d.border_color(k.border_strong)
                    .text_color(k.text_2)
                    .child(n)
            }
        })
}

fn step_row(num: Div, title: String, sub: Option<AnyElement>, right: Option<AnyElement>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .py(px(9.))
        .child(num)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .children(sub),
        )
        .children(right)
}

fn sub_text(s: String, k: &Tokens) -> AnyElement {
    div()
        .mt(px(2.))
        .text_size(px(12.))
        .text_color(k.text_3)
        .child(s)
        .into_any_element()
}

impl Spike {
    pub(crate) fn cli_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let adding = self.settings.cli_adding;
        let add = button("set-cli-add", k, false, adding)
            .child(icon("plus", 13.).text_color(k.text_2))
            .child(t("settings.cli.add"))
            .when(!adding, |b| {
                b.on_click(cx.listener(|this, _, _, cx| this.cli_start_adding(cx)))
            });
        let mut sec = set_section(
            k,
            t("settings.cli.title"),
            Some(t("settings.cli.desc")),
            Some(add.into_any_element()),
        )
        .tag("set-cli");
        if adding {
            sec = sec.child(self.cli_steps(k, cx));
        }
        let list = devices(&self.db);
        if list.is_empty() {
            sec = sec.child(
                div()
                    .tag("set-cli-none")
                    .p(px(18.))
                    .rounded(px(10.))
                    .border_1()
                    .border_dashed()
                    .border_color(k.border_strong)
                    .text_size(px(12.5))
                    .text_color(k.text_3)
                    .child(t("settings.cli.none")),
            );
        } else {
            let mut rows = div().flex().flex_col().gap(px(8.));
            for d in list {
                rows = rows.child(self.cli_device_row(d, k, cx));
            }
            sec = sec.child(rows);
        }
        sec
    }

    fn cli_steps(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let reduce = cx.reduce_motion();
        let has_cmd = installed();
        let request = self.settings.cli_request.as_ref();
        let path = install_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // 1 — 경로 없이 `amber` 로 부를 수 있게
        let install_right = if has_cmd {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.success)
                .child(icon("check", 12.).text_color(k.success))
                .child(t("settings.cli.installed"))
                .into_any_element()
        } else {
            button("set-cli-install", k, false, false)
                .child(t("settings.cli.install"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings.cli_error = crate::cli::install().err();
                    cx.notify();
                }))
                .into_any_element()
        };
        let step1 = step_row(
            step_num("1", has_cmd, !has_cmd, k),
            t("settings.cli.step.install"),
            Some(sub_text(
                t_with("settings.cli.installHint", &[("path", &path)]),
                k,
            )),
            Some(install_right),
        );
        // 2 — 연결 명령
        let copied = self.settings.cli_copied;
        let step2 = step_row(
            step_num("2", request.is_some(), has_cmd && request.is_none(), k),
            t("settings.cli.step.connect"),
            Some(sub_text(t("settings.cli.connectHint"), k)),
            Some(
                button("set-cli-copy", k, false, false)
                    .child(icon(if copied { "check" } else { "copy" }, 13.).text_color(k.text_2))
                    .child(t(if copied {
                        "settings.cli.copied"
                    } else {
                        "settings.cli.copy"
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string("amber connect".into()));
                        this.settings.cli_copied = true;
                        cx.notify();
                    }))
                    .into_any_element(),
            ),
        );
        // 3 — 기다리다가 요청이 들어오면 그 자리에서 고른다
        let step3_body = match request {
            None => div()
                .tag("set-cli-waiting")
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(crate::spinner::spinner(k, reduce))
                .child(t("settings.cli.waiting")),
            Some(r) => div()
                .tag("set-cli-request")
                .flex()
                .items_center()
                .gap(px(12.))
                .p(px(12.))
                .rounded(px(10.))
                .bg(k.surface)
                .border_1()
                .border_color(k.border_strong)
                .child(avatar(r.kind, k))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(program_label(&r.program)),
                        )
                        .child(sub_text(
                            t_with("settings.cli.requestFrom", &[("machine", &r.machine)]),
                            k,
                        )),
                )
                .child(
                    button("set-cli-deny", k, false, false)
                        .child(t("settings.cli.deny"))
                        .on_click(cx.listener(|this, _, _, cx| this.cli_decide(false, cx))),
                )
                .child(
                    primary_button("set-cli-allow", k, false)
                        .child(t("settings.cli.allow"))
                        .on_click(cx.listener(|this, _, _, cx| this.cli_decide(true, cx))),
                ),
        };
        let step3 = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .py(px(9.))
            .child(step_num("3", false, request.is_some(), k))
            .child(div().flex_1().min_w_0().pt(px(2.)).child(step3_body));
        div()
            .tag("set-cli-steps")
            .mb(px(16.))
            .p(px(16.))
            .rounded(px(12.))
            .border_1()
            .border_color(k.glass_edge_accent)
            .bg(crate::settings_view::glass(k.glass_fill))
            .flex()
            .flex_col()
            .when_some(self.settings.cli_error.clone(), |d, e| {
                d.child(note_box(Note::Error, k).mb(px(8.)).child(e))
            })
            .child(step1)
            .child(div().h(px(1.)).bg(k.border))
            .child(step2)
            .child(div().h(px(1.)).bg(k.border))
            .child(step3)
            .child(
                div().flex().justify_end().mt(px(6.)).child(
                    button("set-cli-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cli_stop_adding();
                            cx.notify();
                        })),
                ),
            )
    }

    fn cli_device_row(&self, d: Device, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let when = |ms: i64| local_time(ms).unwrap_or_default();
        let meta = t_with(
            "settings.cli.deviceMeta",
            &[
                ("machine", &d.name),
                ("created", &when(d.created_at)),
                (
                    "used",
                    &d.last_used
                        .map(when)
                        .unwrap_or_else(|| t("settings.cli.never")),
                ),
            ],
        );
        let fresh = self.settings.cli_fresh.as_ref() == Some(&d.id);
        let id = d.id.clone();
        div()
            .tag(format!("set-cli-device-{}", d.id))
            .flex()
            .items_center()
            .gap(px(12.))
            .py(px(10.))
            .px(px(12.))
            .border_1()
            .rounded(px(10.))
            .bg(k.surface)
            .map(|r| {
                if fresh {
                    r.border_color(k.success)
                } else {
                    r.border_color(k.border)
                }
            })
            .child(avatar(&d.kind, k))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(program_label(&d.program)),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(meta),
                    ),
            )
            .child(
                danger_button(format!("set-cli-revoke-{}", d.id), k)
                    .text_color(k.danger)
                    .child(t("settings.cli.revoke"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        crate::cli::revoke(&this.db, &id);
                        cx.notify();
                    })),
            )
    }
}
