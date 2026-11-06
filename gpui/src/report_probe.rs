// 설정 칸들의 바깥 확인 — 리포트(gh 설치/로그인 계정, AI 세션 폴더, claude 에 등록된 MCP 서버)와
// AI 연결(설치된 CLI, 로그인 상태, 연결 시험, Codex 모델 카탈로그).
// 실제 명령(gh, claude mcp list)을 부르는 것은 Core 뿐이다. 시험과 찍기는 Fake 를 꽂아 사용자의 gh, claude 에 닿지 않는다
// (ERD 의 DbBackend 와 같은 이유).

use amber_core::report as core;
use futures::channel::oneshot;

/// gh 와 세션 폴더
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tools {
    /// 찾은 gh (경로, 버전)
    pub gh: Option<(String, String)>,
    pub claude_sessions: bool,
    pub codex_sessions: bool,
}

/// GitHub 준비 상태 — 수집과 같은 준비 단계를 그대로 돌린 결과
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GhState {
    pub ok: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub login: Option<String>,
    /// GH_NOT_FOUND | GH_ACCOUNT | GH_AUTH | GH_SSO | GH_ERROR | REPORT_TIMEOUT
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub login: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub name: String,
    pub connected: bool,
    /// connected | needs_auth | failed | pending | unknown
    pub status: String,
    pub transport: String,
}

impl Server {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "connected": self.connected,
            "status": self.status,
            "transport": self.transport,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Option<Server> {
        Some(Server {
            name: v["name"].as_str()?.to_string(),
            connected: v["connected"].as_bool()?,
            status: v["status"].as_str()?.to_string(),
            transport: v["transport"].as_str()?.to_string(),
        })
    }
}

/// 설치된 AI CLI
#[derive(Clone, Debug, PartialEq)]
pub struct Cli {
    pub id: String,
    pub name: String,
    pub path: String,
    pub version: String,
}

/// CLI 로그인 상태 — logged_in None = 확인할 수 없음
#[derive(Clone, Debug, PartialEq)]
pub struct Auth {
    pub supported: bool,
    pub logged_in: Option<bool>,
}

/// Codex 카탈로그 한 줄
#[derive(Clone, Debug, PartialEq)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

pub type Rx<T> = oneshot::Receiver<T>;

pub trait ReportProbe {
    fn tools(&self) -> Rx<Tools>;
    fn gh_accounts(&self, gh_path: Option<String>) -> Rx<Vec<Account>>;
    fn check_github(&self, path: Option<String>, account: Option<String>) -> Rx<GhState>;
    fn mcp_servers(&self, cli_path: Option<String>) -> Rx<Vec<Server>>;
    fn ai_clis(&self) -> Rx<Vec<Cli>>;
    fn auth_status(&self, provider: Option<String>, cli_path: Option<String>) -> Rx<Auth>;
    /// 연결 시험 — 버전 한 줄 또는 사람이 읽을 오류
    fn health(&self, cli_path: Option<String>) -> Rx<Result<String, String>>;
    fn codex_models(&self) -> Rx<Vec<ModelOption>>;
}

/// 바로 답하는 수신기
pub fn ready<T>(v: T) -> Rx<T> {
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(v);
    rx
}

/// 앱 — amber-core 의 명령을 tokio 에서
pub struct Core;

impl ReportProbe for Core {
    fn ai_clis(&self) -> Rx<Vec<Cli>> {
        crate::ai::spawn(async {
            amber_core::detect::detect_ai_clis()
                .await
                .into_iter()
                .map(|c| Cli {
                    id: c.id,
                    name: c.name,
                    path: c.path,
                    version: c.version,
                })
                .collect()
        })
    }

    fn auth_status(&self, provider: Option<String>, cli_path: Option<String>) -> Rx<Auth> {
        crate::ai::spawn(async move {
            let a = amber_core::auth::ai_auth_status(provider, cli_path).await;
            Auth {
                supported: a.supported,
                logged_in: a.logged_in,
            }
        })
    }

    fn health(&self, cli_path: Option<String>) -> Rx<Result<String, String>> {
        crate::ai::spawn(async move {
            amber_core::ai::ai_health(cli_path)
                .await
                .map_err(|e| crate::ai::err_text(&e))
        })
    }

    fn codex_models(&self) -> Rx<Vec<ModelOption>> {
        crate::ai::spawn(async {
            amber_core::detect::codex_models()
                .await
                .into_iter()
                .map(|m| ModelOption {
                    id: m.id,
                    label: m.label,
                    description: m.description,
                })
                .collect()
        })
    }

    fn tools(&self) -> Rx<Tools> {
        crate::ai::spawn(async {
            let t = core::detect_report_tools().await;
            Tools {
                gh: t.gh.map(|g| (g.path, g.version)),
                claude_sessions: t.claude_sessions,
                codex_sessions: t.codex_sessions,
            }
        })
    }

    fn gh_accounts(&self, gh_path: Option<String>) -> Rx<Vec<Account>> {
        crate::ai::spawn(async move {
            core::report_gh_accounts(gh_path)
                .await
                .into_iter()
                .map(|a| Account {
                    login: a.login,
                    active: a.active,
                })
                .collect()
        })
    }

    fn check_github(&self, path: Option<String>, account: Option<String>) -> Rx<GhState> {
        crate::ai::spawn(async move {
            let c = core::report_check_github(path, account).await;
            GhState {
                ok: c.ok,
                path: c.path,
                version: c.version,
                login: c.login,
                code: c.code,
                message: c.message,
            }
        })
    }

    fn mcp_servers(&self, cli_path: Option<String>) -> Rx<Vec<Server>> {
        crate::ai::spawn(async move {
            core::report_mcp_servers(cli_path)
                .await
                .into_iter()
                .map(|s| Server {
                    name: s.name,
                    connected: s.connected,
                    status: s.status,
                    transport: s.transport,
                })
                .collect()
        })
    }
}

/// 시험, 찍기 — 정해 둔 답
#[derive(Clone, Default)]
pub struct Fake {
    pub tools: Tools,
    pub accounts: Vec<Account>,
    pub gh: GhState,
    pub servers: Vec<Server>,
    pub clis: Vec<Cli>,
    pub auth: Option<Auth>,
    pub version: Option<String>,
    pub codex: Vec<ModelOption>,
}

impl ReportProbe for Fake {
    fn ai_clis(&self) -> Rx<Vec<Cli>> {
        ready(self.clis.clone())
    }

    fn auth_status(&self, _: Option<String>, _: Option<String>) -> Rx<Auth> {
        ready(self.auth.clone().unwrap_or(Auth {
            supported: true,
            logged_in: None,
        }))
    }

    fn health(&self, _: Option<String>) -> Rx<Result<String, String>> {
        ready(self.version.clone().ok_or_else(|| "not found".to_string()))
    }

    fn codex_models(&self) -> Rx<Vec<ModelOption>> {
        ready(self.codex.clone())
    }

    fn tools(&self) -> Rx<Tools> {
        ready(self.tools.clone())
    }

    fn gh_accounts(&self, _: Option<String>) -> Rx<Vec<Account>> {
        ready(self.accounts.clone())
    }

    fn check_github(&self, _: Option<String>, account: Option<String>) -> Rx<GhState> {
        // 없는 계정을 고르면 그 계정 로그인 필요
        let known = account
            .as_deref()
            .is_none_or(|a| a.is_empty() || self.accounts.iter().any(|x| x.login == a));
        ready(if known {
            self.gh.clone()
        } else {
            GhState {
                ok: false,
                code: Some("GH_ACCOUNT".into()),
                ..self.gh.clone()
            }
        })
    }

    fn mcp_servers(&self, _: Option<String>) -> Rx<Vec<Server>> {
        ready(self.servers.clone())
    }
}
