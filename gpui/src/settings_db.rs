// 설정 › 데이터베이스 — 지금 앱 DbSettings.tsx. 연결 목록(이름 + 환경 칩, 종류·주소·계정·스키마 수·폴더 한 줄, 상태 점 + 글자,
// 비밀번호 입력/편집/지우기). 추가와 편집은 ERD 탭의 연결 모달을 그대로 띄우고, 지우기는 ERD 의 [끊기]와 같은 길
// (키체인, 행, 스냅샷 폴더 휴지통)이다. 비밀번호 다시 넣기는 백업을 다른 기기에 복원해 키체인 항목이 없을 때.

use amber_gpui::dbconn::{
    self, DbConnection, conn_status, conn_status_dot, conn_status_label, env_label,
};
use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, danger_button, primary_button};
use crate::note_ai_view::{Note, field_label, hint, note_box};
use crate::settings_view::set_section;
use crate::theme::Tokens;
use crate::tip::tip;

pub struct DbPassword {
    pub conn: DbConnection,
    pub input: Entity<InputState>,
    pub show: bool,
    pub busy: bool,
    pub error: Option<String>,
}

impl Spike {
    fn db_list(&self) -> Vec<DbConnection> {
        dbconn::list_connections(&self.db)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| serde_json::from_value(r).ok())
            .collect()
    }

    pub(crate) fn databases_tab(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let add = button("set-db-add", k, false, false)
            .child(icon("plus", 13.).text_color(k.text_2))
            .child(t("settings.db.add"))
            .on_click(cx.listener(|this, _, window, cx| this.open_conn_modal(None, window, cx)));
        let mut sec = set_section(
            k,
            t("settings.db.title"),
            None,
            Some(add.into_any_element()),
        )
        .tag("set-db");
        if let Some(e) = self.settings.db_error.clone() {
            sec = sec.child(note_box(Note::Error, k).mb(px(12.)).child(e));
        }
        let list = self.db_list();
        if list.is_empty() {
            sec = sec.child(
                div()
                    .mb(px(12.))
                    .max_w(px(470.))
                    .text_size(px(12.5))
                    .line_height(relative(1.6))
                    .text_color(k.text_3)
                    .child(t("settings.db.none")),
            );
        } else {
            let now = chrono::Local::now().timestamp_millis();
            let mut rows = div().flex().flex_col().gap(px(8.));
            for c in list {
                rows = rows.child(self.db_row(c, now, k, cx));
            }
            sec = sec.child(rows);
        }
        sec.child(
            div()
                .mt(px(14.))
                .max_w(px(470.))
                .text_size(px(12.))
                .line_height(relative(1.6))
                .text_color(k.text_3)
                .child(t("settings.db.hint"))
                .child(t("erd.db.hint.readonly")),
        )
    }

    fn db_row(&self, c: DbConnection, now: i64, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let st = conn_status(&c);
        let needs_pw = st == "needsPassword";
        let dot = conn_status_dot(st);
        let mut status = conn_status_label(st);
        if matches!(st, "connected" | "failed")
            && let Some(at) = c.last_sync_at.filter(|v| *v != 0)
        {
            // 지금 앱과 같은 구분자(U+00B7)
            status.push_str(&format!(
                " \u{b7} {}",
                t_with(
                    "erd.db.syncAgo",
                    &[("ago", &crate::notes_view::time_ago(at, now))]
                )
            ));
        }
        let enabled = c.schemas.iter().filter(|s| s.enabled).count();
        let auth = if c.credential_command.is_empty() {
            c.username.clone()
        } else {
            t("erd.db.auth.command")
        };
        let sub = [
            c.kind.clone(),
            format!("{}:{}", c.host, c.port),
            auth,
            t_with("settings.db.schemas", &[("n", &enabled.to_string())]),
            t_with("settings.db.folder", &[("path", &c.folder_path)]),
        ]
        .join(" \u{b7} ");
        let prod = c.env == "prod";
        let id = c.id;
        let (edit, del, pw) = (c.clone(), c.clone(), c.clone());
        let dot_el = div()
            .size(px(7.))
            .flex_none()
            .rounded_full()
            .border(px(1.5))
            .map(|d| match dot {
                "ok" => d.bg(k.success).border_color(k.success),
                "warn" => d.border_color(k.warning),
                "bad" => d.border_color(k.danger),
                _ => d.border_color(k.text_3),
            });
        div()
            .tag(format!("set-db-row-{id}"))
            .flex()
            .items_center()
            .gap(px(12.))
            .py(px(10.))
            .px(px(12.))
            .border_1()
            .rounded(px(8.))
            .bg(k.surface)
            .when(needs_pw, |d| d.border_dashed().border_color(k.warning))
            .when(!needs_pw, |d| d.border_color(k.border))
            .child(
                icon(if needs_pw { "key" } else { "database" }, 16.)
                    .flex_none()
                    .text_color(if needs_pw { k.warning } else { k.text_2 }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(c.name.clone())
                            .child(
                                crate::concepts_view::chip(k)
                                    .text_size(px(11.))
                                    .px(px(7.))
                                    .py(px(1.))
                                    .when(prod, |d| {
                                        d.bg(k.primary_bg)
                                            .border_color(k.primary_bg)
                                            .text_color(k.primary_fg)
                                    })
                                    .child(env_label(&c.env)),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(crate::markdown::MONO)
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(sub),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .whitespace_nowrap()
                            .text_size(px(12.))
                            .text_color(k.text_2)
                            .child(dot_el)
                            .child(status),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .when(needs_pw, |d| {
                                d.child(
                                    button(format!("set-db-pw-{id}"), k, false, false)
                                        .child(t("settings.db.enterPassword"))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.db_open_password(pw.clone(), window, cx)
                                        })),
                                )
                            })
                            .child(
                                button(format!("set-db-edit-{id}"), k, false, false)
                                    .child(t("settings.db.edit"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_conn_modal(Some(edit.clone()), window, cx)
                                    })),
                            )
                            .child(
                                danger_button(format!("set-db-del-{id}"), k)
                                    .tooltip(tip(t("settings.db.deleteTitle")))
                                    .child(icon("trash", 13.).text_color(k.danger))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.settings.db_delete = Some(del.clone());
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn db_open_password(&mut self, c: DbConnection, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).masked(true));
        cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.db_save_password(cx),
            _ => {}
        })
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.settings.db_password = Some(DbPassword {
            conn: c,
            input,
            show: false,
            busy: false,
            error: None,
        });
        cx.notify();
    }

    fn db_save_password(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.settings.db_password.as_mut() else {
            return;
        };
        let value = p.input.read(cx).value().to_string();
        if value.is_empty() || p.busy {
            return;
        }
        p.busy = true;
        p.error = None;
        let ulid = p.conn.ulid.clone();
        let rx = self.erd_db().secret_set(ulid, value);
        cx.spawn(async move |this, cx| {
            let ex = cx.background_executor().clone();
            let r = crate::ai::wait(rx, ex).await;
            let _ = this.update(cx, |s, cx| {
                match r {
                    Some(Ok(())) => {
                        s.settings.db_password = None;
                        // ERD 탭도 새 상태를 보게
                        s.load_erd();
                    }
                    Some(Err(e)) => {
                        if let Some(p) = s.settings.db_password.as_mut() {
                            p.busy = false;
                            p.error = Some(crate::ai::err_text(&e));
                        }
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 지우기 확인 → ERD 의 [끊기]와 같은 길
    pub(crate) fn db_delete_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let c = self.settings.db_delete.as_ref()?;
        let name = c.name.clone();
        let sentence = format!("{name} \u{2014} {}", t("settings.db.deleteBody"));
        let close = || {
            cx.listener(|this, _, _, cx| {
                this.settings.db_delete = None;
                cx.notify();
            })
        };
        Some(crate::dialog::shell(
            k,
            440.,
            t("settings.db.deleteTitle"),
            div()
                .child(StyledText::new(sentence).with_highlights(vec![(
                    0..name.len(),
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                )]))
                .into_any_element(),
            vec![
                button("set-db-del-cancel", k, false, false)
                    .child(t("common.cancel"))
                    .on_click(close()),
                danger_button("set-db-del-confirm", k)
                    .child(t("common.delete"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(c) = this.settings.db_delete.take() {
                            this.settings.db_error = None;
                            this.erd.confirm_disconnect = Some(c);
                            this.erd_disconnect(cx);
                        }
                        cx.notify();
                    })),
            ],
            close(),
        ))
    }

    /// 비밀번호 다시 넣기 — 키체인에만 저장한다
    pub(crate) fn db_password_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.settings.db_password.as_ref()?;
        let empty = p.input.read(cx).value().is_empty();
        let off = empty || p.busy;
        let show = p.show;
        let close = || {
            cx.listener(|this, _, window, cx| {
                this.settings.db_password = None;
                this.focus.focus(window, cx);
                cx.notify();
            })
        };
        let body = div()
            .flex()
            .flex_col()
            .child(field_label(t("erd.db.field.password"), k))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .px(px(11.))
                    .py(px(4.))
                    .text_size(px(14.))
                    .tag("set-db-pw-input")
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&p.input).appearance(false).px_0()),
                    )
                    .child(
                        div()
                            .id("set-db-pw-show")
                            .cursor_pointer()
                            .child(
                                icon(if show { "eye-off" } else { "eye" }, 14.)
                                    .text_color(k.text_2),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(p) = this.settings.db_password.as_mut() {
                                    p.show = !p.show;
                                    let masked = !p.show;
                                    p.input.update(cx, |s, cx| s.set_masked(masked, window, cx));
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(hint(t("erd.db.hint.password"), k))
            .when_some(p.error.clone(), |d, e| {
                d.child(note_box(Note::Error, k).mt(px(12.)).child(e))
            });
        Some(crate::dialog::shell(
            k,
            440.,
            t_with("settings.db.passwordTitle", &[("name", &p.conn.name)]),
            body.into_any_element(),
            vec![
                button("set-db-pw-cancel", k, false, p.busy)
                    .child(t("common.cancel"))
                    .on_click(close()),
                primary_button("set-db-pw-save", k, off)
                    .child(t("common.save"))
                    .when(!off, |b| {
                        b.on_click(cx.listener(|this, _, _, cx| this.db_save_password(cx)))
                    }),
            ],
            close(),
        ))
    }
}
