// 설정 › 데일리 리포트 — 지금 앱 ReportSettings.tsx. 바꾸는 즉시 저장한다(테마처럼).
// 위쪽은 앱이 직접 긁는 소스(GitHub, AI 세션) — 켜고 끄고, 끌어서 순위를 매기고(배열 순서 = 순위), 펼쳐서 소스별 설정.
// 아래 MCP 절은 claude 에 등록된 서버를 그대로 체크 칸으로 낸다(앱이 목록을 고정하지 않는다).
// 이 칸이 보이면 '설정을 거쳤다'로 보고 첫 생성 게이트(onboarded)를 연다.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use amber_gpui::i18n::{t, t_with};
use amber_gpui::report::{self, ReportConfig};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{Value as Json, json};

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Tag, button, checkbox};
use crate::menu::{MenuItem, select_menu};
use crate::note_ai_view::{Note, field_label, hint, note_box};
use crate::report_probe::{Account, GhState, Server, Tools};
use crate::settings_view::{glass, set_section};
use crate::theme::Tokens;
use crate::tip::tip;

/// 앱이 직접 긁는 소스 둘
const ALL_SOURCES: [&str; 2] = ["github", "ai_sessions"];

/// 저장값의 거울 — 화면이 고치고 save 가 그대로 쓴다
#[derive(Clone, Debug, PartialEq)]
pub struct RepCfg {
    pub onboarded: bool,
    pub sources: Vec<(String, bool)>,
    pub github_path: String,
    pub github_repos: Vec<String>,
    pub github_account: String,
    pub sessions_claude: bool,
    pub sessions_codex: bool,
    pub mcp_picked: Vec<String>,
    pub display_name: String,
    pub context: String,
}

impl RepCfg {
    pub fn from_json(v: &Json) -> RepCfg {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let list = |k: &str| {
            v[k].as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        RepCfg {
            onboarded: v["onboarded"].as_bool() == Some(true),
            sources: v["sources"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|x| {
                    (
                        x["id"].as_str().unwrap_or_default().to_string(),
                        x["enabled"].as_bool() == Some(true),
                    )
                })
                .collect(),
            github_path: s("githubPath"),
            github_repos: list("githubRepos"),
            github_account: s("githubAccount"),
            sessions_claude: v["sessionsClaude"].as_bool() != Some(false),
            sessions_codex: v["sessionsCodex"].as_bool() != Some(false),
            mcp_picked: list("mcpPicked"),
            display_name: s("displayName"),
            context: s("context"),
        }
    }

    pub fn save(&self, db: &amber_gpui::db::Db) {
        let c = ReportConfig {
            onboarded: self.onboarded,
            sources: Json::from(
                self.sources
                    .iter()
                    .map(|(id, on)| json!({ "id": id, "enabled": on }))
                    .collect::<Vec<_>>(),
            ),
            github_path: self.github_path.clone(),
            github_repos: self.github_repos.clone(),
            github_account: self.github_account.clone(),
            sessions_claude: self.sessions_claude,
            sessions_codex: self.sessions_codex,
            mcp_picked: Json::from(self.mcp_picked.clone()),
            display_name: self.display_name.clone(),
            context: self.context.clone(),
        };
        let _ = report::save_report_config(db, &c);
    }
}

pub struct RepSettings {
    pub cfg: RepCfg,
    pub tools: Option<Tools>,
    pub accounts: Option<Vec<Account>>,
    pub gh: Option<GhState>,
    pub gh_checking: bool,
    /// 계정을 연달아 바꾸면 늦게 온 옛 결과가 새 결과를 덮지 않게
    gh_seq: u64,
    pub detecting: bool,
    /// 감지 전(None)은 '아직 모름' — 실패(빈 목록)와 다르다. 갱신 중에도 지난 목록을 보여 준다
    pub mcp: Option<Vec<Server>>,
    pub mcp_loading: bool,
    pub is_claude: bool,
    cli_path: Option<String>,
    pub expanded: Option<String>,
    /// 끄는 중인 소스와 행마다의 자리
    pub drag: Option<String>,
    rows: Rc<RefCell<Vec<(String, Bounds<Pixels>)>>>,
    path: Entity<InputState>,
    repos: Entity<InputState>,
    context: Entity<TextareaState>,
    name: Entity<InputState>,
    acct_open: bool,
    acct_anchor: Rc<Cell<Bounds<Pixels>>>,
    /// 방금 복사한 명령(1.4초)
    copied: Option<String>,
    /// 경로 칸의 자리 글 — 찾은 gh 경로(감지가 끝나면 바꾼다)
    ph: String,
}

impl RepSettings {
    /// 닫기 전에 묻기(close_dirty)가 세는 입력칸
    pub(crate) fn field_ids(&self) -> Vec<EntityId> {
        vec![
            self.path.entity_id(),
            self.repos.entity_id(),
            self.context.entity_id(),
            self.name.entity_id(),
        ]
    }
}

fn wait<T: 'static>(
    rx: futures::channel::oneshot::Receiver<T>,
    cx: &mut AsyncApp,
) -> impl std::future::Future<Output = Option<T>> + use<T> {
    crate::ai::wait(rx, cx.background_executor().clone())
}

impl Spike {
    /// 칸이 처음 보였다 — 설정을 읽고(첫 게이트를 연다), 입력칸을 만들고, 감지를 돌린다
    pub(crate) fn mount_report_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut cfg = RepCfg::from_json(&report::load_report_config(&self.db).unwrap_or_default());
        if !cfg.onboarded {
            cfg.onboarded = true;
            cfg.save(&self.db);
        }
        let input =
            |value: String, placeholder: String, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    let mut s = InputState::new(window, cx).placeholder(placeholder);
                    s.set_value(value, window, cx);
                    s
                })
            };
        let path = input(
            cfg.github_path.clone(),
            "/opt/homebrew/bin/gh".into(),
            window,
            cx,
        );
        let repos = input(
            cfg.github_repos.join(", "),
            t("report.gh.repoPlaceholder"),
            window,
            cx,
        );
        let name = input(
            cfg.display_name.clone(),
            t("report.weekly.namePlaceholder"),
            window,
            cx,
        );
        let context = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(4, 12)
                .placeholder(t("report.context.placeholder"));
            s.set_value(cfg.context.clone(), window, cx);
            s
        });
        // 바꾸는 즉시 저장 — 경로는 칸을 떠날 때 다시 확인한다
        cx.subscribe_in(&path, window, |this, s, ev: &InputEvent, _, cx| match ev {
            InputEvent::Change => {
                let v = s.read(cx).value().to_string();
                this.rep_update(cx, |c| c.github_path = v);
            }
            InputEvent::Blur => this.recheck_gh(cx),
            _ => {}
        })
        .detach();
        cx.subscribe_in(&repos, window, |this, s, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let v = s.read(cx).value().to_string();
                let list = v
                    .split(',')
                    .map(|r| amber_gpui::vault::js_trim(r).to_string())
                    .filter(|r| !r.is_empty())
                    .collect();
                this.rep_update(cx, |c| c.github_repos = list);
            }
        })
        .detach();
        cx.subscribe_in(&name, window, |this, s, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let v = s.read(cx).value().to_string();
                this.rep_update(cx, |c| c.display_name = v);
            }
        })
        .detach();
        cx.subscribe_in(&context, window, |this, s, ev: &InputEvent, _, cx| {
            if let InputEvent::Change = ev {
                let v = s.read(cx).value().to_string();
                this.rep_update(cx, |c| c.context = v);
            }
        })
        .detach();
        let app = amber_gpui::ai_config::load_config(&self.db).ok();
        let is_claude = app
            .as_ref()
            .is_some_and(|c| c.provider.as_deref() == Some("claude"));
        let cli_path = app
            .as_ref()
            .map(|c| c.cli_path.clone())
            .filter(|p| !p.is_empty());
        // 지난 감지 결과를 먼저 붙인다 — 오래 걸리는 감지를 기다리지 않고 바로 상태를 보인다
        let cached = if is_claude {
            report::load_mcp_cache(&self.db).ok().and_then(|v| {
                v.as_array()
                    .map(|a| a.iter().filter_map(Server::from_json).collect())
            })
        } else {
            None
        };
        self.settings.rep = Some(RepSettings {
            cfg,
            tools: None,
            accounts: None,
            gh: None,
            gh_checking: false,
            gh_seq: 0,
            detecting: false,
            mcp: cached,
            mcp_loading: false,
            is_claude,
            cli_path,
            expanded: None,
            drag: None,
            rows: Rc::default(),
            path,
            repos,
            context,
            name,
            acct_open: false,
            acct_anchor: Rc::default(),
            copied: None,
            ph: "/opt/homebrew/bin/gh".into(),
        });
        if is_claude {
            self.redetect_mcp(cx);
        }
        self.redetect_tools(cx);
    }

    /// 그리기 전에(창이 있을 때) — 경로 칸의 자리 글을 찾은 gh 경로로
    pub(crate) fn sync_report_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        let want = r
            .tools
            .as_ref()
            .and_then(|t| t.gh.as_ref())
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| "/opt/homebrew/bin/gh".into());
        if r.ph != want {
            r.ph = want.clone();
            r.path
                .update(cx, |s, cx| s.set_placeholder(want, window, cx));
        }
    }

    /// 열린 계정 고르기 판을 닫았으면 true
    pub(crate) fn close_settings_menu(&mut self) -> bool {
        match self.settings.rep.as_mut() {
            Some(r) if r.acct_open => {
                r.acct_open = false;
                true
            }
            _ => false,
        }
    }

    fn rep_update(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut RepCfg)) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        f(&mut r.cfg);
        r.cfg.save(&self.db);
        cx.notify();
    }

    /// 다시 감지 — gh, 세션 폴더 → gh 계정 → GitHub 준비 상태
    fn redetect_tools(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        r.detecting = true;
        let probe = self.settings.probe.clone();
        let rx = probe.tools();
        cx.spawn(async move |this, cx| {
            let Some(tools) = wait(rx, cx).await else {
                return;
            };
            let gh_path = tools.gh.as_ref().map(|(p, _)| p.clone());
            let _ = this.update(cx, |s, cx| {
                if let Some(r) = s.settings.rep.as_mut() {
                    r.tools = Some(tools);
                }
                cx.notify();
            });
            let accounts = wait(probe.gh_accounts(gh_path), cx)
                .await
                .unwrap_or_default();
            let _ = this.update(cx, |s, cx| {
                if let Some(r) = s.settings.rep.as_mut() {
                    r.accounts = Some(accounts);
                    r.detecting = false;
                }
                s.recheck_gh(cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// GitHub 준비 상태 — 수집과 같은 함수로 확인한다(설정에서 초록이면 리포트도 붙는다)
    fn recheck_gh(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        r.gh_seq += 1;
        let seq = r.gh_seq;
        r.gh_checking = true;
        let opt = |s: &str| Some(s.to_string()).filter(|s| !s.is_empty());
        let rx = self
            .settings
            .probe
            .check_github(opt(&r.cfg.github_path), opt(&r.cfg.github_account));
        cx.spawn(async move |this, cx| {
            let got = wait(rx, cx).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(r) = s.settings.rep.as_mut()
                    && r.gh_seq == seq
                {
                    r.gh_checking = false;
                    if let Some(g) = got {
                        r.gh = Some(g);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 등록 서버 다시 감지 — 빈 결과도 캐시에 남긴다(지금 앱과 같다)
    fn redetect_mcp(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        r.mcp_loading = true;
        let rx = self.settings.probe.mcp_servers(r.cli_path.clone());
        cx.spawn(async move |this, cx| {
            let got = wait(rx, cx).await;
            let _ = this.update(cx, |s, cx| {
                if let Some(r) = s.settings.rep.as_mut() {
                    r.mcp_loading = false;
                    if let Some(list) = got {
                        let _ = report::save_mcp_cache(
                            &s.db,
                            &list.iter().map(Server::to_json).collect::<Vec<_>>(),
                        );
                        if let Some(r) = s.settings.rep.as_mut() {
                            r.mcp = Some(list);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn copy_cmd(&mut self, cmd: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(cmd.clone()));
        if let Some(r) = self.settings.rep.as_mut() {
            r.copied = Some(cmd.clone());
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1400))
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Some(r) = s.settings.rep.as_mut()
                    && r.copied.as_deref() == Some(cmd.as_str())
                {
                    r.copied = None;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 끄는 동안 — 커서가 다른 행 위로 가면 배열에서 자리를 바꾼다(놓으면 이미 반영돼 있다)
    fn rep_drag_to(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(r) = self.settings.rep.as_mut() else {
            return;
        };
        let Some(id) = r.drag.clone() else {
            return;
        };
        let over = r
            .rows
            .borrow()
            .iter()
            .find(|(_, b)| b.contains(&p))
            .map(|(i, _)| i.clone());
        let Some(over) = over.filter(|o| *o != id) else {
            return;
        };
        let arr = &mut r.cfg.sources;
        let (Some(from), Some(to)) = (
            arr.iter().position(|(s, _)| *s == id),
            arr.iter().position(|(s, _)| *s == over),
        ) else {
            return;
        };
        let moved = arr.remove(from);
        arr.insert(to, moved);
        cx.notify();
    }

    fn rep_drop(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.settings.rep.as_mut()
            && r.drag.take().is_some()
        {
            r.cfg.save(&self.db);
            cx.notify();
        }
    }

    pub(crate) fn report_settings(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let Some(r) = self.settings.rep.as_ref() else {
            return div();
        };
        let detect = button("rep-redetect", k, false, r.detecting)
            .child(icon("refresh", 13.).text_color(k.text_2))
            .child(t(if r.detecting {
                "report.set.detecting"
            } else {
                "report.set.redetect"
            }))
            .when(!r.detecting, |b| {
                b.on_click(cx.listener(|this, _, _, cx| this.redetect_tools(cx)))
            });
        let enabled: Vec<&String> = r
            .cfg
            .sources
            .iter()
            .filter(|(_, on)| *on)
            .map(|(id, _)| id)
            .collect();
        let rows = r
            .cfg
            .sources
            .iter()
            .filter(|(id, _)| ALL_SOURCES.contains(&id.as_str()));
        r.rows.borrow_mut().clear();
        let mut list = div().flex().flex_col().gap(px(8.));
        for (id, on) in rows {
            let rank = enabled.iter().position(|e| *e == id).map(|i| i + 1);
            list = list.child(self.rep_source_row(r, id, *on, rank, k, cx));
        }
        // 끄는 동안 창 전체에서 움직임과 놓기를 받는다(행 밖으로 나가도 따라간다)
        if r.drag.is_some() {
            let this = cx.entity().downgrade();
            list = list.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let (mv, up) = (this.clone(), this.clone());
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = mv.update(cx, |s, cx| s.rep_drag_to(e.position, cx));
                            }
                        });
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = up.update(cx, |s, cx| s.rep_drop(cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        let field = |label: String, control: AnyElement, h: Option<String>| {
            div()
                .flex()
                .flex_col()
                .child(field_label(label, k))
                .child(control)
                .when_some(h, |d, h| d.child(hint(h, k).max_w(px(470.))))
        };
        set_section(
            k,
            t("report.title"),
            Some(t("report.set.desc")),
            Some(detect.into_any_element()),
        )
        .tag("rep-settings")
        .child(list)
        .child(self.rep_mcp(r, k, cx).mt(px(4.)))
        .child(
            field(
                t("report.context.label"),
                div()
                    .max_w(px(440.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .px(px(3.))
                    .py(px(4.))
                    .font_family(crate::markdown::MONO)
                    .child(
                        Textarea::new(&r.context)
                            .appearance(false)
                            .small()
                            .text_size(px(13.))
                            .line_height(px(20.)),
                    )
                    .tag("rep-context")
                    .into_any_element(),
                Some(t("report.context.hint")),
            )
            .mb(px(16.)),
        )
        .child(field(
            t("report.weekly.nameLabel"),
            input_box(&r.name, k).tag("rep-name").into_any_element(),
            Some(t("report.weekly.nameHint")),
        ))
    }

    fn rep_source_row(
        &self,
        r: &RepSettings,
        id: &str,
        on: bool,
        rank: Option<usize>,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = r.expanded.as_deref() == Some(id);
        let (label, sub) = match id {
            "github" => ("GitHub".to_string(), t("report.sub.github")),
            _ => (t("report.source.aiSessions"), t("report.sub.aiSessions")),
        };
        let (status, ok) = if id == "github" {
            match &r.gh {
                None => (t("report.status.checking"), false),
                Some(g) if g.ok => (format!("@{}", g.login.clone().unwrap_or_default()), true),
                Some(g) => (
                    t(gh_status_key(g.code.as_deref().unwrap_or("GH_ERROR"))),
                    false,
                ),
            }
        } else {
            let has = r
                .tools
                .as_ref()
                .is_some_and(|t| t.claude_sessions || t.codex_sessions);
            if has {
                (t("report.status.detected"), true)
            } else {
                (t("report.status.noSessions"), false)
            }
        };
        let rows = r.rows.clone();
        let row_id = id.to_string();
        let dragging = r.drag.as_deref() == Some(id);
        let (sid, eid) = (id.to_string(), id.to_string());
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .py(px(10.))
            .px(px(12.))
            .child(
                div()
                    .id(SharedString::from(format!("rep-grip-{id}")))
                    .tag(format!("rep-grip-{id}"))
                    .cursor_grab()
                    .tooltip(tip(t("report.set.dragRank")))
                    .child(icon("grip", 14.).text_color(k.text_3))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(r) = this.settings.rep.as_mut() {
                                r.drag = Some(sid.clone());
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                // 순위 — 켠 것만 번호(채움), 끈 것은 대시(아웃라인)
                div()
                    .flex_none()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .when(rank.is_some(), |d| {
                        d.bg(k.primary_bg)
                            .border_color(k.primary_bg)
                            .text_color(k.primary_fg)
                    })
                    .when(rank.is_none(), |d| {
                        d.border_color(k.border_strong).text_color(k.text_3)
                    })
                    .child(rank.map_or("\u{2014}".to_string(), |n| n.to_string())),
            )
            .child(
                checkbox(format!("rep-on-{id}"), k, on, false).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let eid = eid.clone();
                        this.rep_update(cx, |c| {
                            if let Some(s) = c.sources.iter_mut().find(|(i, _)| *i == eid) {
                                s.1 = !s.1;
                            }
                        });
                    },
                )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label)
                            .when(id == "ai_sessions", |d| {
                                d.child(
                                    div()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(k.text_3)
                                        // 지금 앱과 같은 구분자(U+00B7)
                                        .child("CLAUDE \u{b7} CODEX"),
                                )
                            }),
                    )
                    .child(div().text_size(px(11.5)).text_color(k.text_3).child(sub)),
            )
            .child(status_pill(status, ok, k).tag(format!("rep-status-{id}")))
            .child({
                let key = id.to_string();
                div()
                    .id(SharedString::from(format!("rep-expand-{id}")))
                    .tag(format!("rep-expand-{id}"))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(k.text_mix(0.06)))
                    .tooltip(tip(t(if open {
                        "report.set.collapse"
                    } else {
                        "report.set.expand"
                    })))
                    .child(
                        icon(
                            if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            15.,
                        )
                        .text_color(k.text_2),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(r) = this.settings.rep.as_mut() {
                            r.expanded = if r.expanded.as_deref() == Some(&key) {
                                None
                            } else {
                                Some(key.clone())
                            };
                        }
                        cx.notify();
                    }))
            });
        let body = open.then(|| {
            let b = div()
                .p(px(12.))
                .border_t_1()
                .border_dashed()
                .border_color(k.border)
                .bg(k.surface);
            if id == "github" {
                b.child(self.rep_github(r, k, cx))
            } else {
                b.child(self.rep_sessions(r, k, cx))
            }
        });
        div()
            .relative()
            .bg(k.surface_2)
            .border_1()
            .border_color(k.border)
            .when(!on, |d| d.border_dashed())
            .rounded(px(8.))
            .overflow_hidden()
            .when(dragging, |d| d.opacity(0.5))
            .tag(format!("rep-src-{id}"))
            .child(head)
            .children(body)
            .child(
                canvas(
                    move |b, _, _| rows.borrow_mut().push((row_id.clone(), b)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }

    fn rep_github(&self, r: &RepSettings, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let mut d = div().flex().flex_col();
        if let Some(g) = r.gh.as_ref() {
            if !g.ok {
                d = d.child(self.gh_guide(r, g, k, cx).mb(px(12.)));
            } else {
                let version = g.version.clone().unwrap_or_default();
                let version = version
                    .strip_prefix("gh version")
                    .map(str::trim_start)
                    .unwrap_or(&version)
                    .split(' ')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                d = d.child(
                    hint(
                        t_with(
                            "report.gh.ready",
                            &[
                                ("login", g.login.as_deref().unwrap_or("")),
                                ("version", &version),
                                ("path", g.path.as_deref().unwrap_or("")),
                            ],
                        ),
                        k,
                    )
                    .mt_0()
                    .mb(px(12.))
                    .tag("rep-gh-ready"),
                );
            }
        }
        if let Some(accts) = r.accounts.as_ref().filter(|a| !a.is_empty()) {
            let label = if r.cfg.github_account.is_empty() {
                t("report.gh.accountDefault")
            } else {
                acct_label(accts, &r.cfg.github_account)
            };
            let anchor = r.acct_anchor.clone();
            let (hover_fill, hover_edge) = (glass(k.glass_fill_hover), k.glass_edge_hover);
            d = d.child(
                div()
                    .flex()
                    .flex_col()
                    .mb(px(16.))
                    .child(field_label(t("report.gh.accountLabel"), k))
                    .child(
                        div()
                            .id("rep-gh-account")
                            .tag("rep-gh-account")
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
                            .hover(move |s| s.bg(hover_fill).border_color(hover_edge))
                            .child(div().flex_1().min_w_0().overflow_hidden().child(label))
                            .child(icon("chevron-down", 14.).text_color(k.text_3))
                            .child(
                                canvas(move |b, _, _| anchor.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .inset_0(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(r) = this.settings.rep.as_mut() {
                                    r.acct_open = !r.acct_open;
                                }
                                cx.notify();
                            })),
                    )
                    .child(hint(t("report.gh.accountHint"), k).max_w(px(470.)))
                    .children(self.acct_menu(r, accts, k, cx)),
            );
        }
        d.child(
            div()
                .flex()
                .flex_col()
                .mb(px(16.))
                .child(field_label(t("report.gh.pathLabel"), k))
                .child(input_box(&r.path, k).tag("rep-gh-path"))
                .child(hint(t("report.gh.pathHint"), k).max_w(px(470.))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(field_label(t("report.gh.repoLabel"), k))
                .child(input_box(&r.repos, k).tag("rep-gh-repos"))
                .child(hint(t("report.gh.repoHint"), k).max_w(px(470.))),
        )
    }

    fn acct_menu(
        &self,
        r: &RepSettings,
        accts: &[Account],
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Deferred> {
        if !r.acct_open {
            return None;
        }
        let mut values = vec![(String::new(), t("report.gh.accountDefault"))];
        values.extend(
            accts
                .iter()
                .map(|a| (a.login.clone(), acct_label(accts, &a.login))),
        );
        let items = values
            .iter()
            .map(|(v, label)| MenuItem {
                label: label.clone(),
                checked: *v == r.cfg.github_account,
                ..Default::default()
            })
            .collect();
        let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
        Some(select_menu(
            "rep-gh-account-menu",
            r.acct_anchor.get(),
            items,
            k,
            move |i, _, cx| {
                let Some((v, _)) = values.get(i).cloned() else {
                    return;
                };
                let _ = pick.update(cx, |this, cx| {
                    if let Some(r) = this.settings.rep.as_mut() {
                        r.acct_open = false;
                    }
                    this.rep_update(cx, |c| c.github_account = v);
                    this.recheck_gh(cx);
                });
            },
            move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    if let Some(r) = this.settings.rep.as_mut() {
                        r.acct_open = false;
                    }
                    cx.notify();
                });
            },
        ))
    }

    /// GitHub 이 안 붙을 때 할 일 — 단계마다 터미널 명령 하나(복사), 마치면 다시 확인
    fn gh_guide(&self, r: &RepSettings, g: &GhState, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let custom = !amber_gpui::vault::js_trim(&r.cfg.github_path).is_empty();
        let account = r.cfg.github_account.clone();
        let steps: Vec<(String, Option<&str>)> = match g.code.as_deref() {
            Some("GH_NOT_FOUND") if custom => vec![(t("report.gh.fix.badPath"), None)],
            Some("GH_NOT_FOUND") => vec![
                (t("report.gh.fix.install"), Some("brew install gh")),
                (t("report.gh.fix.login"), Some("gh auth login")),
            ],
            Some("GH_ACCOUNT") => vec![
                (
                    t_with("report.gh.fix.accountLogin", &[("account", &account)]),
                    Some("gh auth login"),
                ),
                (t("report.gh.fix.accountPick"), None),
            ],
            Some("GH_AUTH") => vec![(t("report.gh.fix.login"), Some("gh auth login"))],
            Some("GH_SSO") => vec![(
                t("report.gh.fix.sso"),
                Some("gh auth refresh -h github.com"),
            )],
            _ => vec![(
                g.message
                    .clone()
                    .unwrap_or_else(|| t("report.status.ghFailed")),
                None,
            )],
        };
        let mut ol = div().flex().flex_col().gap(px(8.));
        for (i, (text, cmd)) in steps.into_iter().enumerate() {
            ol = ol.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(div().flex_none().w(px(12.)).child(format!("{}.", i + 1)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(text)
                            .when_some(cmd, |d, cmd| d.child(self.cmd_row(r, cmd, k, cx))),
                    ),
            );
        }
        let checking = r.gh_checking;
        note_box(Note::Warn, k).tag("rep-gh-guide").child(ol).child(
            div()
                .mt(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(hint(t("report.gh.fix.after"), k).mt_0())
                .child(
                    button("rep-gh-recheck", k, false, checking)
                        .child(icon("refresh", 13.).text_color(k.text_2))
                        .child(t(if checking {
                            "report.status.checking"
                        } else {
                            "report.gh.fix.recheck"
                        }))
                        .when(!checking, |b| {
                            b.on_click(cx.listener(|this, _, _, cx| this.recheck_gh(cx)))
                        }),
                ),
        )
    }

    fn cmd_row(&self, r: &RepSettings, cmd: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let copied = r.copied.as_deref() == Some(cmd);
        let c = cmd.to_string();
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(cmd_code(cmd, k).flex_1().min_w_0())
            .child(
                div()
                    .id(SharedString::from(format!("rep-copy-{cmd}")))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(k.glass_edge)
                    .bg(glass(k.glass_fill))
                    .cursor_pointer()
                    .tooltip(tip(t(if copied {
                        "report.gh.copied"
                    } else {
                        "report.copy"
                    })))
                    .child(icon(if copied { "check" } else { "copy" }, 13.).text_color(k.text_2))
                    .on_click(cx.listener(move |this, _, _, cx| this.copy_cmd(c.clone(), cx))),
            )
    }

    fn rep_sessions(&self, r: &RepSettings, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let tools = r.tools.clone().unwrap_or_default();
        let row =
            |id: &'static str,
             on: bool,
             name: &'static str,
             found: bool,
             cx: &mut Context<Self>| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(checkbox(id, k, on, false).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.rep_update(cx, |c| {
                                if id == "rep-sess-claude" {
                                    c.sessions_claude = !c.sessions_claude;
                                } else {
                                    c.sessions_codex = !c.sessions_codex;
                                }
                            });
                        },
                    )))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(4.))
                            .text_size(px(13.))
                            .child(name)
                            .child(div().text_size(px(11.)).text_color(k.text_3).child(t(
                                if found {
                                    "report.status.detected"
                                } else {
                                    "report.sess.noFolder"
                                },
                            ))),
                    )
            };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(row(
                "rep-sess-claude",
                r.cfg.sessions_claude,
                "Claude Code",
                tools.claude_sessions,
                cx,
            ))
            .child(row(
                "rep-sess-codex",
                r.cfg.sessions_codex,
                "Codex",
                tools.codex_sessions,
                cx,
            ))
    }

    /// MCP 서버 절 — 순위 없이 켜고 끄는 것뿐. claude 전용
    fn rep_mcp(&self, r: &RepSettings, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let off = r.mcp_loading || !r.is_claude;
        let action = button("rep-mcp-redetect", k, false, off)
            .child(icon("refresh", 13.).text_color(k.text_2))
            .child(t(if r.mcp_loading {
                "report.set.detecting"
            } else {
                "report.mcp.redetect"
            }))
            .when(!off, |b| {
                b.on_click(cx.listener(|this, _, _, cx| this.redetect_mcp(cx)))
            });
        let sec = set_section(
            k,
            t("report.mcp.sectionTitle"),
            Some(t("report.mcp.sectionDesc")),
            Some(action.into_any_element()),
        )
        .tag("rep-mcp");
        if !r.is_claude {
            return sec.child(
                div()
                    .max_w(px(470.))
                    .text_size(px(12.))
                    .text_color(k.text_3)
                    .child(
                        StyledText::new(format!(
                            "{}{}{}",
                            t_with("report.mcp.claudeOnlyPre", &[("name", "MCP")]),
                            t("report.mcp.claudeOnlyLink"),
                            t("report.mcp.claudeOnlyPost")
                        ))
                        .with_highlights({
                            let pre = t_with("report.mcp.claudeOnlyPre", &[("name", "MCP")]).len();
                            let link = t("report.mcp.claudeOnlyLink").len();
                            vec![(
                                pre..pre + link,
                                HighlightStyle {
                                    font_weight: Some(FontWeight::BOLD),
                                    ..Default::default()
                                },
                            )]
                        }),
                    ),
            );
        }
        let Some(found) = r.mcp.as_ref() else {
            return sec.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .py(px(12.))
                    .child(crate::spinner::spinner(k, cx.reduce_motion()))
                    .child(hint(t("report.mcp.searching"), k).mt_0()),
            );
        };
        // 목록에 없는데 저장돼 있는 이름도 행으로 — 안 보이면 끌 수가 없다(감지 실패, 서버 삭제)
        let mut rows: Vec<Server> = found.clone();
        for n in &r.cfg.mcp_picked {
            if !found.iter().any(|s| s.name == *n) {
                rows.push(Server {
                    name: n.clone(),
                    connected: false,
                    status: "unknown".into(),
                    transport: String::new(),
                });
            }
        }
        if rows.is_empty() {
            return sec.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(hint(t("report.mcp.noneGuide"), k).mt_0().mb(px(2.)))
                    .child(cmd_code(&t("report.mcp.addStep"), k))
                    .child(cmd_code(&t("report.mcp.authStep"), k)),
            );
        }
        let mut list = div().flex().flex_col().gap(px(6.));
        for (i, srv) in rows.into_iter().enumerate() {
            let checked = r.cfg.mcp_picked.contains(&srv.name);
            let status = if srv.connected {
                t("report.mcpStatus.connected")
            } else {
                match srv.status.as_str() {
                    "needs_auth" => t("report.mcpStatus.needsAuth"),
                    "failed" => t("report.mcpStatus.failed"),
                    "pending" => t("report.mcpStatus.pending"),
                    "connected" => t("report.mcpStatus.connected"),
                    _ => t("report.status.notConnected"),
                }
            };
            let name = srv.name.clone();
            let mut cb = checkbox(format!("rep-mcp-{i}"), k, checked, !srv.connected);
            if srv.connected {
                cb = cb.on_click(cx.listener(move |this, _, _, cx| {
                    let name = name.clone();
                    this.rep_update(cx, |c| {
                        if c.mcp_picked.contains(&name) {
                            c.mcp_picked.retain(|n| *n != name);
                        } else {
                            c.mcp_picked.push(name);
                        }
                    });
                }));
            }
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .py(px(8.))
                    .px(px(12.))
                    .bg(k.surface_2)
                    .border_1()
                    .border_color(k.border)
                    .when(!srv.connected, |d| d.border_dashed())
                    .rounded(px(8.))
                    .child(cb)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(!srv.connected, |d| d.text_color(k.text_2))
                                    .child(srv.name.clone()),
                            )
                            .when(!srv.connected, |d| {
                                d.child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(k.text_3)
                                        .child(t("report.mcp.connectHint")),
                                )
                            }),
                    )
                    .child(status_pill(status, srv.connected, k)),
            );
        }
        sec.child(list)
            .child(hint(t("report.mcp.tokenHint"), k).mt(px(8.)).mb(px(16.)))
    }
}

fn gh_status_key(code: &str) -> &'static str {
    match code {
        "GH_NOT_FOUND" => "report.status.ghMissing",
        "GH_ACCOUNT" => "report.status.ghAccount",
        "GH_AUTH" => "report.status.ghLogin",
        "GH_SSO" => "report.status.ghSso",
        "REPORT_TIMEOUT" => "report.status.ghTimeout",
        _ => "report.status.ghFailed",
    }
}

/// "login · 활성" — 지금 앱과 같은 구분자(U+00B7)
fn acct_label(accts: &[Account], login: &str) -> String {
    match accts.iter().find(|a| a.login == login) {
        Some(a) if a.active => format!("{} \u{b7} {}", a.login, t("report.gh.accountActive")),
        _ => login.to_string(),
    }
}

/// .rep-status — 붙었으면 초록 선과 글자(면적이 작은 자리라 면은 칠하지 않는다)
fn status_pill(text: String, ok: bool, k: &Tokens) -> Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(11.))
        .px(px(9.))
        .py(px(2.))
        .rounded_full()
        .border_1()
        .text_color(if ok { k.success_text } else { k.text_3 })
        .border_color(if ok { k.success } else { k.border })
        .child(text)
}

/// .rep-mcp-cmd — 복사용 명령 한 줄
fn cmd_code(cmd: &str, k: &Tokens) -> Div {
    div()
        .font_family(crate::markdown::MONO)
        .text_size(px(11.5))
        .text_color(k.text)
        .bg(k.surface_2)
        .border_1()
        .border_color(k.border)
        .rounded(px(6.))
        .py(px(7.))
        .px(px(10.))
        .child(cmd.to_string())
}

/// .input — 강한 테, 바탕, 폭은 440 까지
fn input_box(e: &Entity<InputState>, k: &Tokens) -> Div {
    div()
        .max_w(px(440.))
        .rounded(px(6.))
        .border_1()
        .border_color(k.border_strong)
        .bg(k.surface)
        .px(px(11.))
        .py(px(4.))
        .text_size(px(14.))
        .child(Input::new(e).appearance(false).px_0())
}
