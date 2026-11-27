// 설정 › CLI — 터미널에서 Amber 를 다룰 수 있게 연결한 기기 목록과 [연결 해제], 에이전트 지침에 붙여 넣을 명령 안내.
// 연결은 여기서 시작하지 않는다 — 터미널이 `amber connect` 를 실행하면 앱 위에 허용 창(cli_request_modal)이 뜨고,
// 확인 코드를 맞춰 본 사람이 [허용]을 눌러야 기기가 목록에 들어온다.

use amber_gpui::i18n::{t, t_with};
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::cli::{Device, devices, local_time, setup_text};
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::settings_view::set_section;
use crate::theme::Tokens;

impl Spike {
    pub(crate) fn cli_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut sec = set_section(
            k,
            t("settings.cli.title"),
            Some(t("settings.cli.desc")),
            None,
        )
        .tag("set-cli");
        let list = devices(&self.db);
        if list.is_empty() {
            sec = sec.child(
                div()
                    .tag("set-cli-none")
                    .max_w(px(470.))
                    .text_size(px(12.5))
                    .line_height(relative(1.6))
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
        let usage = setup_text();
        let copied = self.settings.cli_copied;
        let copy = button("set-cli-copy-usage", k, false, false)
            .child(icon(if copied { "check" } else { "copy" }, 13.).text_color(k.text_2))
            .child(t(if copied {
                "settings.cli.copied"
            } else {
                "settings.cli.copy"
            }))
            .on_click(cx.listener({
                let usage = usage.clone();
                move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(usage.clone()));
                    this.settings.cli_copied = true;
                    cx.notify();
                }
            }));
        div().flex().flex_col().child(sec).child(
            set_section(
                k,
                t("settings.cli.usageTitle"),
                Some(t("settings.cli.usageDesc")),
                Some(copy.into_any_element()),
            )
            .tag("set-cli-usage")
            .mt(px(20.))
            .pt(px(20.))
            .border_t_1()
            .border_color(k.border)
            .child(
                div()
                    .max_w(px(560.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .px(px(11.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .font_family(crate::markdown::MONO)
                    .text_size(px(12.))
                    .text_color(k.text_2)
                    .children(usage.lines().map(|l| div().child(l.to_string()))),
            ),
        )
    }

    fn cli_device_row(&self, d: Device, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let when = |ms: i64| local_time(ms).unwrap_or_default();
        let meta = t_with(
            "settings.cli.deviceMeta",
            &[
                ("created", &when(d.created_at)),
                (
                    "used",
                    &d.last_used
                        .map(when)
                        .unwrap_or_else(|| t("settings.cli.never")),
                ),
            ],
        );
        let id = d.id.clone();
        div()
            .tag(format!("set-cli-device-{}", d.id))
            .flex()
            .items_center()
            .gap(px(12.))
            .py(px(10.))
            .px(px(12.))
            .border_1()
            .border_color(k.border)
            .rounded(px(8.))
            .bg(k.surface)
            .child(icon("key", 16.).flex_none().text_color(k.text_2))
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
                            .child(d.name),
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

    /// CLI 연결 요청 — 설정이 닫혀 있어도 앱 위에 뜬다. 닫기(X)는 거절
    pub(crate) fn cli_request_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let r = self.settings.cli_request.as_ref()?;
        let deny = || cx.listener(|this, _, _, cx| this.cli_decide(false, cx));
        Some(
            crate::dialog::shell(
                k,
                460.,
                t("settings.cli.requestTitle"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(t_with("settings.cli.requestBody", &[("name", &r.name)]))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(k.text_3)
                                    .child(t("settings.cli.requestCode")),
                            )
                            .child(
                                div()
                                    .tag("cli-request-code")
                                    .font_family(crate::markdown::MONO)
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(r.code.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.6))
                            .text_color(k.text_3)
                            .child(t("settings.cli.requestHint")),
                    )
                    .into_any_element(),
                vec![
                    button("cli-request-deny", k, false, false)
                        .child(t("settings.cli.deny"))
                        .on_click(deny()),
                    primary_button("cli-request-allow", k, false)
                        .child(t("settings.cli.allow"))
                        .on_click(cx.listener(|this, _, _, cx| this.cli_decide(true, cx))),
                ],
                deny(),
            )
            .tag("cli-request"),
        )
    }
}
