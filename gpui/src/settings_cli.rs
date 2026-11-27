// 설정 › CLI — 터미널에서 Amber 를 다룰 기기를 연결하고 끊는다. [기기 연결]을 누르면 2분짜리 일회용 코드와 실행할 명령이
// 뜨고, 그 명령이 코드를 맞히면 기기가 목록에 들어온다. 아래에는 에이전트 지침에 붙여 넣을 명령 안내.

use amber_gpui::i18n::{t, t_with};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::cli::{Device, devices, exe_path, local_time, setup_text};
use crate::day_card::{Tag, button, danger_button};
use crate::note_ai_view::hint;
use crate::settings_view::set_section;
use crate::theme::Tokens;

/// 글이 여러 줄인 고정폭 상자(명령, 안내)
fn mono_box(text: &str, k: &Tokens) -> Div {
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
        .children(text.lines().map(|l| div().child(l.to_string())))
}

impl Spike {
    fn cli_copy_button(
        &self,
        id: &'static str,
        which: &'static str,
        text: String,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let copied = self.settings.cli_copied == Some(which);
        button(id, k, false, false)
            .child(icon(if copied { "check" } else { "copy" }, 13.).text_color(k.text_2))
            .child(t(if copied {
                "settings.cli.copied"
            } else {
                "settings.cli.copy"
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                this.settings.cli_copied = Some(which);
                cx.notify();
            }))
    }

    pub(crate) fn cli_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let pairing = self.settings.cli_pair.as_ref();
        let connect = button("set-cli-pair", k, false, pairing.is_some())
            .child(icon("plus", 13.).text_color(k.text_2))
            .child(t("settings.cli.pair"))
            .when(pairing.is_none(), |b| {
                b.on_click(cx.listener(|this, _, _, cx| this.cli_open_pairing(cx)))
            });
        let mut sec = set_section(
            k,
            t("settings.cli.title"),
            Some(t("settings.cli.desc")),
            Some(connect.into_any_element()),
        )
        .tag("set-cli");
        if let Some(p) = pairing {
            let command = format!("{} connect {}", exe_path(), p.code);
            sec = sec.child(
                div()
                    .tag("set-cli-pairing")
                    .mb(px(14.))
                    .p(px(14.))
                    .max_w(px(560.))
                    .rounded(px(8.))
                    .border_1()
                    .border_dashed()
                    .border_color(k.border_strong)
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(k.text_2)
                            .child(t("settings.cli.pairTitle")),
                    )
                    .child(
                        div()
                            .tag("set-cli-code")
                            .font_family(crate::markdown::MONO)
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child(p.code.clone()),
                    )
                    .child(mono_box(&command, k))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(self.cli_copy_button(
                                "set-cli-copy-command",
                                "command",
                                command.clone(),
                                k,
                                cx,
                            ))
                            .child(
                                button("set-cli-pair-cancel", k, false, false)
                                    .child(t("common.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings.cli_pair = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(hint(t("settings.cli.pairHint"), k).mt_0()),
            );
        }
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
        div().flex().flex_col().child(sec).child(
            set_section(
                k,
                t("settings.cli.usageTitle"),
                Some(t("settings.cli.usageDesc")),
                Some(
                    self.cli_copy_button("set-cli-copy-usage", "usage", usage.clone(), k, cx)
                        .into_any_element(),
                ),
            )
            .tag("set-cli-usage")
            .mt(px(20.))
            .pt(px(20.))
            .border_t_1()
            .border_color(k.border)
            .child(mono_box(&usage, k)),
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
}
