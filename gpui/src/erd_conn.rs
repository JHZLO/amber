// DB 연결 모달 — 지금 앱 DbConnectionModal.tsx. 접속 정보(1/2) → 스키마 고르기(2/2). 새 연결과 편집이 같이 쓴다.
// 비밀번호는 저장하는 순간 한 번 키체인에 넣는다 — 프로필(SQLite)엔 들어가지 않고, 편집은 "키체인에 저장됨"만 보인다.
// 동기화는 여기서 하지 않는다: 저장하면 ERD 트리가 행마다 진행을 보이며 스키마를 읽는다(모달이 기다리지 않게).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use amber_core::ai::AiError;
use amber_core::dbconn::{DbProfile, DbTestResult};
use amber_gpui::dbconn::{DbConnection, DbSchemaPref, env_label, folder_name_for};
use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, primary_button};
use crate::menu::{MenuItem, select_menu};
use crate::note_ai_view::{field_label, hint};
use crate::theme::Tokens;

const ENVS: [&str; 3] = ["dev", "staging", "prod"];
const TLS: [&str; 3] = ["preferred", "required", "disabled"];
/// 6열 격자의 칸 사이
const GAP: f32 = 14.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    Form,
    Schemas,
}

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Env,
    Tls,
    Auth,
}

pub struct ConnModal {
    /// 편집 대상. None = 새 연결
    pub editing: Option<DbConnection>,
    pub step: Step,
    name: Entity<InputState>,
    host: Entity<InputState>,
    port: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    command: Entity<InputState>,
    schema_text: Entity<TextareaState>,
    pub env: String,
    pub tls: String,
    /// 계정을 스키마마다 명령으로 받는가(아니면 키체인의 비밀번호 하나)
    pub by_command: bool,
    /// 편집에서 비밀번호 칸을 열었나 — 기본은 "키체인에 저장됨"
    change_pw: bool,
    show_pw: bool,
    pub testing: bool,
    /// 확인 실패는 저장 실패와 자리가 다르다(확인 칸 안)
    pub test_error: Option<String>,
    pub test: Option<DbTestResult>,
    pub error: Option<String>,
    pub prefs: Vec<DbSchemaPref>,
    labels: HashMap<String, Entity<InputState>>,
    /// 감사(_aud) 표 포함 — 새 연결의 기본. 편집에선 건드렸을 때만 모든 스키마에 다시 적용한다
    include_audit: bool,
    audit_touched: bool,
    pub busy: bool,
    pub prod_confirm: bool,
    pick: Option<Pick>,
    anchors: [Rc<Cell<Bounds<Pixels>>>; 3],
    form_w: Rc<Cell<f32>>,
    task: Option<Task<()>>,
}

fn port_of(s: &str) -> Option<u16> {
    let s = s.trim();
    if s.is_empty() || s.len() > 5 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>()
        .ok()
        .filter(|n| (1..=65535).contains(n))
        .map(|n| n as u16)
}

/// 계정 명령 연결의 스키마 칸 → 이름 목록. 쉼표, 공백, 줄바꿈 어느 것으로 나눠도 된다
fn schema_list_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in text.split(|c: char| c == ',' || c.is_whitespace()) {
        if !s.is_empty() && !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    }
    out
}

/// 확인에 실패한 스키마의 이유 — 행이 이미 이름을 말하니 detail 앞의 "스키마: " 는 뗀다
fn fail_reason(name: &str, e: &AiError) -> String {
    let prefix = format!("{name}: ");
    let detail = e
        .detail
        .as_deref()
        .map(|d| d.strip_prefix(&prefix).unwrap_or(d).to_string());
    crate::ai::err_text(&AiError {
        code: e.code.clone(),
        message: e.message.clone(),
        detail,
    })
}

fn tls_label(v: &str) -> String {
    t(&format!("erd.db.tls.{v}"))
}

fn auth_label(by_command: bool) -> String {
    t(if by_command {
        "erd.db.auth.command"
    } else {
        "erd.db.auth.password"
    })
}

impl ConnModal {
    fn value(e: &Entity<InputState>, cx: &App) -> String {
        e.read(cx).value().to_string()
    }

    fn pw_stored(&self) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|c| c.credential_command.is_empty())
    }

    fn schema_list(&self, cx: &App) -> Vec<String> {
        schema_list_of(&self.schema_text.read(cx).value())
    }

    fn fields_valid(&self, cx: &App) -> bool {
        let v = |e: &Entity<InputState>| Self::value(e, cx);
        !v(&self.name).trim().is_empty()
            && !v(&self.host).trim().is_empty()
            && port_of(&v(&self.port)).is_some()
            && if self.by_command {
                !v(&self.command).trim().is_empty() && !self.schema_list(cx).is_empty()
            } else {
                !v(&self.user).trim().is_empty()
                    && (self.pw_stored() && !self.change_pw || !v(&self.password).is_empty())
            }
    }

    fn enabled_count(&self) -> usize {
        self.prefs.iter().filter(|p| p.enabled).count()
    }

    fn failed_count(&self) -> usize {
        self.test.as_ref().map_or(0, |r| {
            r.schemas.iter().filter(|s| s.error.is_some()).count()
        })
    }

    fn ok_count(&self) -> usize {
        self.test.as_ref().map_or(0, |r| r.schemas.len()) - self.failed_count()
    }

    /// 저장할 스키마 목록 — 새 연결이거나 감사 체크를 건드렸으면 모든 스키마에 적용한다
    fn prefs_to_save(&self) -> Vec<DbSchemaPref> {
        if self.editing.is_some() && !self.audit_touched {
            return self.prefs.clone();
        }
        self.prefs
            .iter()
            .map(|p| DbSchemaPref {
                audit: (!self.include_audit).then_some(false),
                ..p.clone()
            })
            .collect()
    }
}

impl Spike {
    /// 모달을 연다 — 편집이면 그 연결의 값으로
    pub(crate) fn open_conn_modal(
        &mut self,
        conn: Option<DbConnection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let c = conn.as_ref();
        let mut input = |v: String, ph: Option<String>, masked: bool, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut s = InputState::new(window, cx).default_value(v).masked(masked);
                if let Some(ph) = ph {
                    s = s.placeholder(ph);
                }
                s
            })
        };
        let name = input(
            c.map(|c| c.name.clone()).unwrap_or_default(),
            Some(t("erd.db.field.namePh")),
            false,
            cx,
        );
        let host = input(
            c.map_or("127.0.0.1".into(), |c| c.host.clone()),
            None,
            false,
            cx,
        );
        let port = input(
            c.map_or("3306".into(), |c| c.port.to_string()),
            None,
            false,
            cx,
        );
        let user = input(
            c.map(|c| c.username.clone()).unwrap_or_default(),
            None,
            false,
            cx,
        );
        let password = input(String::new(), None, true, cx);
        let command = input(
            c.map(|c| c.credential_command.clone()).unwrap_or_default(),
            Some("~/bin/db-credential {schema}".into()),
            false,
            cx,
        );
        let by_command = c.is_some_and(|c| !c.credential_command.is_empty());
        let schema_text = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(3, 3)
                .placeholder(t("erd.db.field.schemaListPh"));
            if by_command && let Some(c) = c {
                let names: Vec<&str> = c.schemas.iter().map(|p| p.name.as_str()).collect();
                s.set_value(names.join(", "), window, cx);
            }
            s
        });
        // 접속 정보를 고치면 직전 확인은 다른 서버 얘기다 — 다시 확인하게 한다. Enter = 확인
        for (e, touch) in [
            (&name, false),
            (&host, true),
            (&port, true),
            (&user, true),
            (&password, true),
            (&command, true),
        ] {
            cx.subscribe_in(e, window, move |this, _, ev: &InputEvent, _, cx| {
                match ev {
                    InputEvent::Change => {
                        if let Some(m) = this.erd.conn.as_mut()
                            && touch
                        {
                            m.test = None;
                            m.test_error = None;
                        }
                    }
                    InputEvent::PressEnter { .. } => this.conn_run_test(cx),
                    _ => {}
                }
                cx.notify();
            })
            .detach();
        }
        cx.subscribe_in(&schema_text, window, |this, _, ev: &InputEvent, _, cx| {
            if let (InputEvent::Change, Some(m)) = (ev, this.erd.conn.as_mut()) {
                m.test = None;
                m.test_error = None;
            }
            cx.notify();
        })
        .detach();
        if conn.is_none() {
            name.update(cx, |s, cx| s.focus(window, cx));
        }
        let include_audit = !c.is_some_and(|c| {
            !c.schemas.is_empty() && c.schemas.iter().all(|p| p.audit == Some(false))
        });
        self.erd.conn = Some(ConnModal {
            env: c.map_or("dev".into(), |c| c.env.clone()),
            tls: c.map_or("preferred".into(), |c| c.tls.clone()),
            prefs: c.map(|c| c.schemas.clone()).unwrap_or_default(),
            editing: conn,
            step: Step::Form,
            name,
            host,
            port,
            user,
            password,
            command,
            schema_text,
            by_command,
            change_pw: false,
            show_pw: false,
            testing: false,
            test_error: None,
            test: None,
            error: None,
            labels: HashMap::new(),
            include_audit,
            audit_touched: false,
            busy: false,
            prod_confirm: false,
            pick: None,
            anchors: Default::default(),
            form_w: Rc::new(Cell::new(0.)),
            task: None,
        });
    }

    /// 닫는다 — 입력칸이 사라지니 포커스를 뿌리로(남아 있으면 그 뒤 키가 뿌리에 닿지 않는다)
    pub(crate) fn close_conn_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.erd.conn = None;
        self.focus.focus(window, cx);
    }

    fn conn_profile(&self, cx: &App) -> Option<DbProfile> {
        let m = self.erd.conn.as_ref()?;
        let v = |e: &Entity<InputState>| ConnModal::value(e, cx);
        Some(DbProfile {
            ulid: m.editing.as_ref().map_or("new".into(), |c| c.ulid.clone()),
            kind: "mysql".into(),
            host: v(&m.host).trim().to_string(),
            port: port_of(&v(&m.port)).unwrap_or(3306),
            username: v(&m.user).trim().to_string(),
            tls: m.tls.clone(),
            credential_command: if m.by_command {
                v(&m.command).trim().to_string()
            } else {
                String::new()
            },
        })
    }

    /// 연결 확인 — 붙어 보고 스키마 목록을 받는다. 이미 있던 설정(표시명, 체크)은 지키고 새로 보인 스키마만
    /// 기본값으로(표가 있으면 켠다)
    pub(crate) fn conn_run_test(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.conn_profile(cx) else {
            return;
        };
        let Some(m) = self.erd.conn.as_ref() else {
            return;
        };
        if !m.fields_valid(cx) || m.testing {
            return;
        }
        // 편집 중 비밀번호를 안 바꿨으면 Rust 가 키체인에서 읽는다
        let password = (!m.by_command && !(m.pw_stored() && !m.change_pw))
            .then(|| ConnModal::value(&m.password, cx));
        let schemas = m.by_command.then(|| m.schema_list(cx));
        let rx = self.erd_db().test(profile, password, schemas);
        let m = self.erd.conn.as_mut().unwrap();
        m.testing = true;
        m.test_error = None;
        m.task = Some(cx.spawn(async move |this, cx| {
            let r = crate::ai::wait(rx, cx.background_executor().clone()).await;
            let _ = this.update(cx, |s, cx| {
                let Some(m) = s.erd.conn.as_mut() else {
                    return;
                };
                m.testing = false;
                match r {
                    Some(Ok(res)) => {
                        m.prefs = res
                            .schemas
                            .iter()
                            .map(|sc| {
                                m.prefs
                                    .iter()
                                    .find(|p| p.name == sc.name)
                                    .cloned()
                                    .unwrap_or(DbSchemaPref {
                                        name: sc.name.clone(),
                                        label: String::new(),
                                        enabled: sc.tables > 0,
                                        audit: None,
                                        starred: None,
                                    })
                            })
                            .collect();
                        m.test = Some(res);
                    }
                    Some(Err(e)) => m.test_error = Some(crate::ai::err_text(&e)),
                    None => {}
                }
                cx.notify();
            });
        }));
    }

    /// [다음] — 운영 연결을 처음 더하면 한 번 묻는다
    fn conn_go_schemas(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.erd.conn.as_mut() else {
            return;
        };
        if m.test.is_none() {
            return;
        }
        if m.env == "prod" && m.editing.is_none() {
            m.prod_confirm = true;
            return;
        }
        self.conn_enter_schemas(window, cx);
    }

    /// 스키마 단계 — 표시명 칸을 만든다(창이 있어야 만들어진다)
    pub(crate) fn conn_enter_schemas(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 폼의 입력칸이 사라진다 — 포커스를 뿌리로
        self.focus.focus(window, cx);
        let Some(m) = self.erd.conn.as_mut() else {
            return;
        };
        m.step = Step::Schemas;
        let names: Vec<(String, String)> = m
            .prefs
            .iter()
            .filter(|p| !m.labels.contains_key(&p.name))
            .map(|p| (p.name.clone(), p.label.clone()))
            .collect();
        for (name, label) in names {
            let e = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(label)
                    .placeholder(t("erd.db.schemas.labelPh"))
            });
            let key = name.clone();
            cx.subscribe_in(&e, window, move |this, e, ev: &InputEvent, _, cx| {
                if let (InputEvent::Change, Some(m)) = (ev, this.erd.conn.as_mut()) {
                    let v = e.read(cx).value().to_string();
                    if let Some(p) = m.prefs.iter_mut().find(|p| p.name == key) {
                        p.label = v;
                    }
                    cx.notify();
                }
            })
            .detach();
            self.erd.conn.as_mut().unwrap().labels.insert(name, e);
        }
    }

    /// 새 연결의 스냅샷 폴더 이름 — 디스크의 폴더나 다른 연결의 folder_path 와 겹치면 -2, -3… (소문자로 견준다:
    /// macOS 파일시스템은 대소문자를 가리지 않는다)
    fn conn_pick_folder(&self, name: &str) -> String {
        let base = folder_name_for(name);
        let taken: Vec<String> = self
            .erd
            .connections
            .iter()
            .flatten()
            .map(|c| c.folder_path.to_lowercase())
            .collect();
        let files = amber_gpui::vault::Tree::new(&self.db, &amber_gpui::vault::ERD);
        let mut folder = base.clone();
        let mut i = 2;
        while taken.contains(&folder.to_lowercase()) || files.file_exists(&folder) {
            folder = format!("{base}-{i}");
            i += 1;
        }
        folder
    }

    /// 저장 — 편집은 행을 고치고 키체인을 맞춘다. 새 연결은 행을 만들고 비밀번호를 넣는다(못 넣으면 행을 되돌린다)
    pub(crate) fn conn_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.erd.conn.as_ref() else {
            return;
        };
        let v = |e: &Entity<InputState>| ConnModal::value(e, cx);
        let Some(port) = port_of(&v(&m.port)) else {
            return;
        };
        if m.busy || !m.fields_valid(cx) {
            return;
        }
        if m.editing.is_none() && m.enabled_count() == 0 {
            self.erd.conn.as_mut().unwrap().error = Some(t("erd.db.schemas.none"));
            return;
        }
        let by_command = m.by_command;
        let (name, host, user, command, password) = (
            v(&m.name),
            v(&m.host),
            if by_command {
                String::new()
            } else {
                v(&m.user)
            },
            if by_command {
                v(&m.command)
            } else {
                String::new()
            },
            v(&m.password),
        );
        let schemas = serde_json::to_value(m.prefs_to_save()).unwrap_or_default();
        let (env, tls) = (m.env.clone(), m.tls.clone());
        let pw_stored = m.pw_stored();
        let change_pw = m.change_pw;
        let tested = m.test.is_some() || m.audit_touched;
        let editing = m.editing.clone();
        let db = self.erd_db();
        let saved: Result<(DbConnection, Option<futures::channel::oneshot::Receiver<_>>), String> =
            match &editing {
                Some(c) => amber_gpui::dbconn::update_connection(
                    &self.db,
                    c.id,
                    &amber_gpui::dbconn::ConnectionPatch {
                        name: Some(name),
                        env: Some(env),
                        host: Some(host),
                        port: Some(port as f64),
                        username: Some(user),
                        tls: Some(tls),
                        credential_command: Some(command),
                        // 확인을 안 했으면 스키마 목록은 손대지 않는다(이름만 고치는 길)
                        schemas: tested.then_some(schemas),
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    // 계정 명령으로 바꿨으면 키체인의 비밀번호는 더 쓰지 않는다 — 남기지 않는다
                    let rx = if by_command {
                        pw_stored.then(|| db.secret_delete(c.ulid.clone()))
                    } else if !password.is_empty() && (change_pw || !pw_stored) {
                        Some(db.secret_set(c.ulid.clone(), password.clone()))
                    } else {
                        None
                    };
                    let fresh = amber_gpui::dbconn::get_connection(&self.db, c.id)
                        .map_err(|e| e.to_string())?
                        .and_then(|r| serde_json::from_value::<DbConnection>(r).ok())
                        .ok_or_else(|| "connection vanished".to_string())?;
                    Ok((fresh, rx))
                }),
                None => {
                    let folder = self.conn_pick_folder(&name);
                    amber_gpui::dbconn::create_connection(
                        &self.db,
                        &amber_gpui::dbconn::ConnectionInput {
                            name,
                            kind: "mysql".into(),
                            env,
                            host,
                            port: port as f64,
                            username: user,
                            tls,
                            credential_command: command,
                            folder_path: folder,
                            schemas,
                        },
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|row| {
                        serde_json::from_value::<DbConnection>(row).map_err(|e| e.to_string())
                    })
                    .map(|c| {
                        let rx =
                            (!by_command).then(|| db.secret_set(c.ulid.clone(), password.clone()));
                        (c, rx)
                    })
                }
            };
        let m = self.erd.conn.as_mut().unwrap();
        let (saved, rx) = match saved {
            Ok(x) => x,
            Err(e) => {
                m.error = Some(e);
                return;
            }
        };
        m.busy = true;
        m.error = None;
        let is_new = editing.is_none();
        m.task = Some(cx.spawn_in(window, async move |this, cx| {
            let secret = match rx {
                Some(rx) => crate::ai::wait(rx, cx.background_executor().clone())
                    .await
                    .unwrap_or(Ok(())),
                None => Ok(()),
            };
            // 비밀번호 없는 새 프로필은 쓸모가 없다 — 행을 되돌려 반쪽 상태를 남기지 않는다
            if let Err(e) = secret {
                if is_new {
                    let undo = this.update(cx, |s, cx| s.erd_delete_connection(&saved, cx));
                    if let Ok(undo) = undo {
                        let _ = undo.await;
                    }
                }
                let _ = this.update(cx, |s, cx| {
                    if let Some(m) = s.erd.conn.as_mut() {
                        m.busy = false;
                        m.error = Some(crate::ai::err_text(&e));
                    }
                    s.load_erd();
                    cx.notify();
                });
                return;
            }
            let _ = this.update_in(cx, |s, window, cx| {
                s.close_conn_modal(window, cx);
                // 저장되면 펼치고, 스냅샷이 없는 스키마를 채운다
                s.erd.expanded.insert(format!("c{}", saved.id));
                s.erd_enter(cx);
                cx.notify();
            });
        }));
    }

    /// .modal — 단계 라벨, 오류, 폼 또는 스키마 표, 바닥 단추. 운영 연결 확인은 그 위에 한 겹 더
    pub(crate) fn erd_conn_view(
        &self,
        k: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let m = self.erd.conn.as_ref()?;
        let editing = m.editing.is_some();
        let valid = m.fields_valid(cx);
        let eyebrow = div()
            .mt(px(-4.))
            .mb(px(14.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .text_color(k.text_3)
            .child(
                t(if m.step == Step::Form {
                    "erd.db.modal.step1"
                } else {
                    "erd.db.modal.step2"
                })
                .to_uppercase(),
            )
            .when(m.step == Step::Form, |d| {
                d.child(div().opacity(0.72).child("MYSQL"))
            });
        let mut body = div()
            .flex()
            .flex_col()
            .child(eyebrow)
            .when_some(m.error.clone(), |d, e| {
                d.child(
                    crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                        .tag("erd-conn-error")
                        .mb(px(12.))
                        .child(e),
                )
            });
        body = match m.step {
            Step::Form => body.child(self.conn_form(m, valid, k, window, cx)),
            Step::Schemas => body.child(self.conn_schemas(m, k, cx)),
        };
        let mut left = Vec::new();
        let mut right = vec![
            button("erd-conn-cancel", k, false, m.busy)
                .child(t("common.cancel"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_conn_modal(window, cx);
                    cx.notify();
                })),
        ];
        let next = |primary: bool, cx: &mut Context<Self>| {
            let ok = m.test.is_some() && !m.busy;
            let b = if primary {
                primary_button("erd-conn-next", k, !ok)
            } else {
                button("erd-conn-next", k, false, !ok)
            };
            b.child(t("erd.db.next"))
                .child(icon("chevron-right", 13.).text_color(if primary {
                    k.surface
                } else {
                    k.text_2
                }))
                .when(ok, |d| {
                    d.on_click(cx.listener(|this, _, window, cx| {
                        this.conn_go_schemas(window, cx);
                        cx.notify();
                    }))
                })
        };
        let working = if m.busy {
            Some(t("diagrams.working"))
        } else {
            None
        };
        match m.step {
            Step::Form if editing => {
                right.push(next(false, cx));
                let ok = valid && !m.busy;
                right.push(
                    primary_button("erd-conn-save", k, !ok)
                        .child(working.clone().unwrap_or_else(|| t("erd.db.saveChanges")))
                        .when(ok, |d| {
                            d.on_click(cx.listener(|this, _, window, cx| {
                                this.conn_save(window, cx);
                                cx.notify();
                            }))
                        }),
                );
            }
            Step::Form => right.push(next(true, cx)),
            Step::Schemas => {
                left.push(
                    button("erd-conn-back", k, false, m.busy)
                        .child(icon("chevron-left", 13.).text_color(k.text_2))
                        .child(t("erd.db.back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.erd.conn.as_mut() {
                                m.step = Step::Form;
                            }
                            cx.notify();
                        })),
                );
                let ok = !m.busy && (editing || m.enabled_count() > 0);
                right.push(
                    primary_button("erd-conn-save", k, !ok)
                        .child(working.unwrap_or_else(|| {
                            if editing {
                                t("erd.db.saveChanges")
                            } else {
                                t_with(
                                    "erd.db.connectAndSync",
                                    &[("n", &m.enabled_count().to_string())],
                                )
                            }
                        }))
                        .when(ok, |d| {
                            d.on_click(cx.listener(|this, _, window, cx| {
                                this.conn_save(window, cx);
                                cx.notify();
                            }))
                        }),
                );
            }
        }
        let modal = crate::dialog::shell_fixed(
            k,
            640.,
            t(if editing {
                "erd.db.modal.editTitle"
            } else {
                "erd.db.modal.title"
            }),
            body.into_any_element(),
            left,
            right,
            cx.listener(|this, _, window, cx| {
                this.close_conn_modal(window, cx);
                cx.notify();
            }),
        );
        let confirm = m.prod_confirm.then(|| {
            crate::dialog::shell(
                k,
                440.,
                t("erd.db.prod.confirmTitle"),
                div().child(t("erd.db.prod.confirmBody")).into_any_element(),
                vec![
                    button("erd-prod-cancel", k, false, false)
                        .child(t("common.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.erd.conn.as_mut() {
                                m.prod_confirm = false;
                            }
                            cx.notify();
                        })),
                    primary_button("erd-prod-ok", k, false)
                        .child(t("erd.db.prod.confirmOk"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(m) = this.erd.conn.as_mut() {
                                m.prod_confirm = false;
                            }
                            this.conn_enter_schemas(window, cx);
                            cx.notify();
                        })),
                ],
                cx.listener(|this, _, _, cx| {
                    if let Some(m) = this.erd.conn.as_mut() {
                        m.prod_confirm = false;
                    }
                    cx.notify();
                }),
            )
        });
        Some(
            div()
                .absolute()
                .inset_0()
                .child(modal)
                .children(confirm)
                .children(self.conn_select_menu(m, k, cx))
                .into_any_element(),
        )
    }

    /// .select-trigger — 고른 값과 꺾쇠. 누르면 그 아래에 판이 뜬다
    fn conn_select(
        &self,
        id: &'static str,
        which: Pick,
        label: String,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let m = self.erd.conn.as_ref().unwrap();
        let anchor = m.anchors[which as usize].clone();
        let (hover_fill, hover_edge) = (
            crate::note_ai_view::glass(k.glass_fill_hover),
            k.glass_edge_hover,
        );
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
            .bg(crate::note_ai_view::glass(k.glass_fill))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(k.text)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_fill).border_color(hover_edge))
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
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(m) = this.erd.conn.as_mut() {
                    m.pick = if m.pick == Some(which) {
                        None
                    } else {
                        Some(which)
                    };
                }
                cx.notify();
            }))
    }

    fn conn_select_menu(
        &self,
        m: &ConnModal,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Deferred> {
        let which = m.pick?;
        let values: Vec<(String, String)> = match which {
            Pick::Env => ENVS.iter().map(|e| (e.to_string(), env_label(e))).collect(),
            Pick::Tls => TLS.iter().map(|v| (v.to_string(), tls_label(v))).collect(),
            Pick::Auth => vec![
                ("password".into(), auth_label(false)),
                ("command".into(), auth_label(true)),
            ],
        };
        let cur = match which {
            Pick::Env => m.env.clone(),
            Pick::Tls => m.tls.clone(),
            Pick::Auth => if m.by_command { "command" } else { "password" }.into(),
        };
        let items = values
            .iter()
            .map(|(v, label)| MenuItem {
                label: label.clone(),
                checked: *v == cur,
                ..Default::default()
            })
            .collect();
        let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
        Some(select_menu(
            "erd-conn-menu",
            m.anchors[which as usize].get(),
            items,
            k,
            move |i, _, cx| {
                let Some((v, _)) = values.get(i).cloned() else {
                    return;
                };
                let _ = pick.update(cx, |this, cx| {
                    if let Some(m) = this.erd.conn.as_mut() {
                        m.pick = None;
                        match which {
                            Pick::Env => m.env = v,
                            // TLS, 계정 방식은 접속 정보다 — 직전 확인을 버린다
                            Pick::Tls | Pick::Auth => {
                                if which == Pick::Tls {
                                    m.tls = v;
                                } else {
                                    m.by_command = v == "command";
                                }
                                m.test = None;
                                m.test_error = None;
                            }
                        }
                    }
                    cx.notify();
                });
            },
            move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    if let Some(m) = this.erd.conn.as_mut() {
                        m.pick = None;
                    }
                    cx.notify();
                });
            },
        ))
    }

    /// 접속 정보 — 칸 전부가 하나의 6열 격자 위에 선다(span 칸 = span 열 + (span-1) 사이)
    fn conn_form(
        &self,
        m: &ConnModal,
        valid: bool,
        k: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let _ = window;
        let w = m.form_w.get();
        let col = ((w - GAP * 5.) / 6.).max(0.);
        let span = |n: f32| px(col * n + GAP * (n - 1.));
        let boxed = |e: &Entity<InputState>, tag: &'static str| {
            div()
                .tag(tag)
                .rounded(px(6.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .px(px(11.))
                .py(px(4.))
                .text_size(px(14.))
                .child(Input::new(e).appearance(false).px_0())
        };
        let field = |n: f32, label: String, control: AnyElement| {
            div()
                .w(span(n))
                .flex_none()
                .flex()
                .flex_col()
                .child(field_label(label, k))
                .child(control)
        };
        let row = || div().w_full().flex().gap(px(GAP));
        let form_hint = |text: String| hint(text, k).mt(px(-6. + 0.));
        let pw_stored = m.pw_stored();
        let password: AnyElement = if pw_stored && !m.change_pw {
            div()
                .h(px(34.))
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(10.))
                .pr(px(4.))
                .border_1()
                .border_dashed()
                .border_color(k.border_strong)
                .rounded(px(6.))
                .text_size(px(13.))
                .text_color(k.text_2)
                .overflow_hidden()
                .child(icon("key", 14.).flex_none().text_color(k.text_2))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(t("erd.db.password.stored")),
                )
                .child(
                    button("erd-conn-change-pw", k, false, false)
                        .flex_none()
                        .child(t("erd.db.password.change"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.erd.conn.as_mut() {
                                m.change_pw = true;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            let show = m.show_pw;
            div()
                .relative()
                .child(boxed(&m.password, "erd-conn-password").pr(px(34.)))
                .child(
                    div()
                        .id("erd-conn-show-pw")
                        .tag("erd-conn-show-pw")
                        .absolute()
                        .right(px(5.))
                        .top(px(5.))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .tooltip(crate::tip::tip(t(if show {
                            "erd.db.password.hide"
                        } else {
                            "erd.db.password.show"
                        })))
                        .child(icon(if show { "eye-off" } else { "eye" }, 14.).text_color(k.text_2))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(m) = this.erd.conn.as_mut() {
                                m.show_pw = !m.show_pw;
                                let masked = !m.show_pw;
                                m.password
                                    .update(cx, |s, cx| s.set_masked(masked, window, cx));
                            }
                            cx.notify();
                        })),
                )
                .into_any_element()
        };
        let account: Vec<Div> = if m.by_command {
            vec![
                row()
                    .child(field(
                        2.,
                        t("erd.db.field.auth"),
                        self.conn_select("erd-conn-auth", Pick::Auth, auth_label(true), k, cx)
                            .into_any_element(),
                    ))
                    .child(field(
                        4.,
                        t("erd.db.field.command"),
                        boxed(&m.command, "erd-conn-command")
                            .font_family(crate::markdown::MONO)
                            .into_any_element(),
                    )),
                row().child(field(
                    6.,
                    t("erd.db.field.schemaList"),
                    div()
                        .tag("erd-conn-schema-list")
                        .rounded(px(6.))
                        .border_1()
                        .border_color(k.border_strong)
                        .bg(k.surface)
                        .px(px(3.))
                        .py(px(7.))
                        .child(
                            Textarea::new(&m.schema_text)
                                .appearance(false)
                                .small()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(13.))
                                .line_height(relative(1.6)),
                        )
                        .into_any_element(),
                )),
                form_hint(t("erd.db.hint.command")),
            ]
        } else {
            vec![
                row()
                    .child(field(
                        2.,
                        t("erd.db.field.auth"),
                        self.conn_select("erd-conn-auth", Pick::Auth, auth_label(false), k, cx)
                            .into_any_element(),
                    ))
                    .child(field(
                        2.,
                        t("erd.db.field.user"),
                        boxed(&m.user, "erd-conn-user").into_any_element(),
                    ))
                    .child(field(2., t("erd.db.field.password"), password)),
                form_hint(t("erd.db.hint.account")),
            ]
        };
        let failed = m.failed_count();
        let (bg, bar, ink, strong) = if m.test.is_some() {
            (k.success_soft, k.success, k.success_text, k.success_text)
        } else if m.test_error.is_some() {
            (k.danger_soft, k.danger, k.danger_text, k.danger_text)
        } else {
            (k.surface_2, k.border_strong, k.text_3, k.text)
        };
        let (title, text): (String, String) = if m.testing {
            (t("erd.db.test.testing"), String::new())
        } else if let Some(r) = &m.test {
            let mut s = t_with(
                "erd.db.testOk",
                &[
                    ("server", &r.server),
                    ("n", &m.ok_count().to_string()),
                    ("ms", &r.latency_ms.to_string()),
                ],
            );
            if failed > 0 {
                s += "\n";
                s += &t_with("erd.db.test.someFailed", &[("n", &failed.to_string())]);
            }
            (t("erd.db.test.ok"), s)
        } else if let Some(e) = &m.test_error {
            (t("erd.db.test.failed"), e.clone())
        } else {
            (t("erd.db.test.idle"), t("erd.db.test.idleHint"))
        };
        let can_test = valid && !m.testing && !m.busy;
        let test_box = div()
            .tag("erd-conn-test")
            .mt(px(18.))
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(14.))
            .py(px(12.))
            .rounded(px(6.))
            .bg(bg)
            .border_l(px(3.))
            .border_color(bar)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(ink)
                    .child(
                        div()
                            .mb(px(2.))
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(strong)
                            .child(title),
                    )
                    .children(
                        text.split('\n')
                            .map(|l| div().child(l.to_string()))
                            .collect::<Vec<_>>(),
                    ),
            )
            .child(
                button("erd-conn-test-run", k, false, !can_test)
                    .child(t(if m.testing {
                        "erd.db.testing"
                    } else if m.test.is_some() || m.test_error.is_some() {
                        "erd.db.test.again"
                    } else {
                        "erd.db.test"
                    }))
                    .when(can_test, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| {
                            this.conn_run_test(cx);
                            cx.notify();
                        }))
                    }),
            );
        let fw = m.form_w.clone();
        div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .child(
                canvas(
                    move |b, window, _| {
                        let w = f32::from(b.size.width);
                        if fw.get() != w {
                            fw.set(w);
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                row()
                    .child(field(
                        4.,
                        t("erd.db.field.name"),
                        boxed(&m.name, "erd-conn-name").into_any_element(),
                    ))
                    .child(field(
                        2.,
                        t("erd.db.field.env"),
                        self.conn_select("erd-conn-env", Pick::Env, env_label(&m.env), k, cx)
                            .into_any_element(),
                    )),
            )
            .child(
                row()
                    .child(field(
                        3.,
                        t("erd.db.field.host"),
                        boxed(&m.host, "erd-conn-host").into_any_element(),
                    ))
                    .child(field(
                        1.,
                        t("erd.db.field.port"),
                        boxed(&m.port, "erd-conn-port").into_any_element(),
                    ))
                    .child(field(
                        2.,
                        t("erd.db.field.tls"),
                        self.conn_select("erd-conn-tls", Pick::Tls, tls_label(&m.tls), k, cx)
                            .into_any_element(),
                    )),
            )
            .child(form_hint(t("erd.db.hint.host")))
            .children(account)
            .child(test_box)
    }

    /// 스키마 고르기 — 요약 한 줄, 표(체크, 이름, 표시명 또는 실패 이유, 표 수), 감사 표 포함
    fn conn_schemas(&self, m: &ConnModal, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let Some(r) = &m.test else {
            return div();
        };
        let v = |e: &Entity<InputState>| ConnModal::value(e, cx);
        let summary = t_with(
            "erd.db.schemas.summary",
            &[
                ("name", v(&m.name).trim()),
                ("host", v(&m.host).trim()),
                ("port", &v(&m.port)),
                (
                    "user",
                    &if m.by_command {
                        t("erd.db.auth.command")
                    } else {
                        v(&m.user).trim().to_string()
                    },
                ),
            ],
        );
        let toggle = |name: String| {
            move |this: &mut Spike, _: &ClickEvent, _: &mut Window, cx: &mut Context<Spike>| {
                if let Some(m) = this.erd.conn.as_mut()
                    && let Some(p) = m.prefs.iter_mut().find(|p| p.name == name)
                {
                    p.enabled = !p.enabled;
                }
                cx.notify();
            }
        };
        let rows: Vec<Div> = r
            .schemas
            .iter()
            .map(|s| {
                let on = m
                    .prefs
                    .iter()
                    .find(|p| p.name == s.name)
                    .is_some_and(|p| p.enabled);
                let mut row = div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(12.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(k.border)
                    .child(
                        crate::day_card::checkbox(
                            format!("erd-conn-schema-{}", s.name),
                            k,
                            on,
                            false,
                        )
                        .on_click(cx.listener(toggle(s.name.clone()))),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "erd-conn-schema-name-{}",
                                s.name
                            )))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(crate::markdown::MONO)
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if on { k.text } else { k.text_3 })
                            .cursor_pointer()
                            .child(s.name.clone())
                            .on_click(cx.listener(toggle(s.name.clone()))),
                    );
                row = match &s.error {
                    // 붙지 못한 스키마는 표시명과 표 수 대신 이유를 둔다
                    Some(e) => row.child(
                        div()
                            .w(px(140. + 56. + 10.))
                            .flex_none()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(11.5))
                            .text_color(k.text_3)
                            .child(fail_reason(&s.name, e)),
                    ),
                    None => row
                        .child(
                            div()
                                .w(px(140.))
                                .flex_none()
                                .h(px(26.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(k.border_strong)
                                .bg(k.surface)
                                .text_size(px(12.))
                                .when(!on, |d| d.opacity(0.45))
                                .children(m.labels.get(&s.name).map(|e| {
                                    Input::new(e).appearance(false).px_0().small().disabled(!on)
                                })),
                        )
                        .child(
                            div()
                                .w(px(56.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .text_size(px(11.5))
                                .text_color(k.text_3)
                                .child(s.tables.to_string()),
                        ),
                };
                row
            })
            .collect();
        let audit_click =
            |this: &mut Spike, _: &ClickEvent, _: &mut Window, cx: &mut Context<Spike>| {
                if let Some(m) = this.erd.conn.as_mut() {
                    m.include_audit = !m.include_audit;
                    m.audit_touched = true;
                }
                cx.notify();
            };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(12.))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(8.))
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(summary)
                    // 원래 화면의 구분점(<span>·</span>)을 그대로 둔다
                    .child("\u{00B7}")
                    .child(t_with(
                        "erd.db.schemas.readable",
                        &[("n", &m.ok_count().to_string())],
                    )),
            )
            .child(
                div()
                    .tag("erd-conn-schemas")
                    .flex()
                    .flex_col()
                    .bg(k.surface)
                    .border_1()
                    .border_color(k.border)
                    .rounded(px(10.))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(12.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(k.border)
                            .text_size(px(11.5))
                            .text_color(k.text_3)
                            .child(div().w(px(20.)))
                            .child(div().flex_1().child(t("erd.db.schemas.col.schema")))
                            .child(div().w(px(140.)).child(t("erd.db.schemas.col.label")))
                            .child(
                                div()
                                    .w(px(56.))
                                    .flex()
                                    .justify_end()
                                    .child(t("erd.db.schemas.col.tables")),
                            ),
                    )
                    .children(rows),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        crate::day_card::checkbox("erd-conn-audit", k, m.include_audit, false)
                            .on_click(cx.listener(audit_click)),
                    )
                    .child(
                        div()
                            .id("erd-conn-audit-label")
                            .cursor_pointer()
                            .text_size(px(13.))
                            .text_color(k.text_2)
                            .child(t("erd.db.audit.modalLabel"))
                            .on_click(cx.listener(audit_click)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{port_of, schema_list_of};

    #[test]
    fn port_and_schema_list_read_like_the_app() {
        assert_eq!(port_of(" 3306 "), Some(3306));
        assert_eq!(port_of("0"), None);
        assert_eq!(port_of("65536"), None);
        assert_eq!(port_of("33a"), None);
        assert_eq!(port_of("123456"), None);
        assert_eq!(schema_list_of("a, b\nc  a,,d"), ["a", "b", "c", "d"]);
    }
}
