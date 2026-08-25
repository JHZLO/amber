// AI CLI 로그인 브리지 — 만료된 인증을 앱 안에서 다시 맺는다.
//
// 왜 앱 안인가: 토큰은 몇 주에 한 번 조용히 만료되고, 그때 AI 가 붙은 기능이 전부(리포트·개념·
// 노트) 같이 멈춘다. 그런데 화면에 남는 건 "로그인이 필요하다" 한 줄뿐이고 실제 해결은 터미널
// 이었다. 고치는 자리는 멈춘 자리 옆에 있어야 한다.
//
// 왜 가능한가: `claude auth login` 은 TTY 를 요구하지 않는다 — stdout 으로 인증 URL 을 뱉고
// stdin 으로 붙여넣은 코드를 읽는다. 앱이 그 자식 프로세스를 붙들고 있으면
// (URL → 브라우저, 코드 → stdin) 모달 안에서 흐름이 닫힌다. codex 는 브라우저 콜백으로 스스로
// 끝나 코드 입력 단계가 없다. gemini 는 대화형 TUI 뿐이라 지원하지 않는다.
//
// **자격증명은 만지지 않는다.** 코드는 자식 stdin 으로 흘려보낼 뿐이고 토큰 저장은 CLI 가 제
// 저장소(macOS 키체인)에 한다 — Amber 의 DB·설정에는 아무것도 남지 않는다.

use serde::Serialize;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, Command};
use tokio::time::timeout;

use crate::ai::{default_binary, provider_kind, AiError, LiveGuard, ProviderKind};

/// 상태 조회 상한 — 설정 모달 스피너가 멈추지 않게 detect.rs 의 probe 와 같은 값.
const STATUS_TIMEOUT_SECS: u64 = 8;
/// 로그인 프로세스 수명 상한. 모달을 닫고 잊어도 `claude auth login` 이 영영 살아 있지 않게.
const LOGIN_TIMEOUT_SECS: u64 = 600;
/// 진행 중인 로그인 자식의 취소 키 — 앱 종료 시 ai::kill_all 이 같이 정리한다.
const LOGIN_KEY: &str = "ai-auth-login";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    /// 이 프로바이더를 앱 안에서 로그인시킬 수 있는가 (아니면 터미널 안내로 내려간다)
    pub supported: bool,
    /// Some(true/false) = 확인됨, None = 확인 불가(옛 CLI·예상 밖 출력)
    pub logged_in: Option<bool>,
    /// CLI 가 말한 그대로 한 줄 — 진단용
    pub detail: String,
}

impl AuthStatus {
    fn unsupported() -> Self {
        Self { supported: false, logged_in: None, detail: String::new() }
    }
}

/// 로그인 진행 중 프론트로 흘리는 이벤트. 출력 조각과 종료를 한 채널로 보낸다 —
/// 끝났는지 알려면 프론트가 따로 폴링해야 하는 구조를 만들지 않기 위해서.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoginEvent {
    /// CLI 가 stdout 에 쓴 조각 그대로 (URL·프롬프트·에러). 줄바꿈이 없어도 흘려보낸다 —
    /// "Paste code here if prompted > " 는 개행 없이 오기 때문에 줄 단위로 읽으면 멈춘다.
    Output { text: String },
    /// 자식이 끝났다. status 로 성공/실패가 갈린다.
    Done { status: AuthStatus },
}

/// 살아 있는 로그인 세션. 코드를 받아 stdin 에 넣어야 하므로 프로세스를 붙들고 있어야 한다.
/// 동시에 두 개를 열 이유가 없어 하나만 유지한다(새로 시작하면 이전 것을 죽인다).
struct LoginSession {
    pid: u32,
    stdin: ChildStdin,
}

static LOGIN: OnceLock<Mutex<Option<LoginSession>>> = OnceLock::new();

fn session() -> &'static Mutex<Option<LoginSession>> {
    LOGIN.get_or_init(|| Mutex::new(None))
}

fn take_session() -> Option<LoginSession> {
    session().lock().ok().and_then(|mut s| s.take())
}

fn program_for(provider: Option<&str>, cli_path: Option<String>) -> (ProviderKind, String) {
    let kind = provider_kind(provider);
    let program = cli_path
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| default_binary(kind).to_string());
    (kind, program)
}

/// 로그인 명령 인자. None = 그 CLI 는 앱 안 로그인을 지원하지 않는다.
fn login_args(kind: ProviderKind) -> Option<&'static [&'static str]> {
    match kind {
        // --claudeai: 구독 로그인(기본값이지만 CLI 가 물어보지 않게 명시한다)
        ProviderKind::Claude => Some(&["auth", "login", "--claudeai"]),
        ProviderKind::Codex => Some(&["login"]),
        ProviderKind::Gemini => None,
    }
}

/// 현재 인증 상태. 만료를 에러로 마주치기 전에 설정·모달에서 먼저 보여 준다.
#[tauri::command]
pub async fn ai_auth_status(provider: Option<String>, cli_path: Option<String>) -> AuthStatus {
    let (kind, program) = program_for(provider.as_deref(), cli_path);
    if login_args(kind).is_none() {
        return AuthStatus::unsupported();
    }

    let args: &[&str] = match kind {
        ProviderKind::Claude => &["auth", "status"],
        _ => &["login", "status"],
    };
    let Ok(Ok(out)) = timeout(
        Duration::from_secs(STATUS_TIMEOUT_SECS),
        Command::new(&program).args(args).kill_on_drop(true).output(),
    )
    .await
    else {
        return AuthStatus { supported: true, logged_in: None, detail: String::new() };
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    let logged_in = match kind {
        // claude 는 JSON 을 준다: {"loggedIn": bool, "authMethod": …}
        ProviderKind::Claude => serde_json::from_str::<serde_json::Value>(stdout.trim())
            .ok()
            .and_then(|v| v.get("loggedIn").and_then(|b| b.as_bool())),
        // codex 는 사람이 읽는 한 줄 + 종료 코드가 신호다
        _ => Some(out.status.success()),
    };
    let detail = match kind {
        ProviderKind::Claude => serde_json::from_str::<serde_json::Value>(stdout.trim())
            .ok()
            .and_then(|v| v.get("authMethod").and_then(|s| s.as_str()).map(String::from))
            .unwrap_or_default(),
        _ => stdout.lines().next().unwrap_or("").trim().to_string(),
    };
    AuthStatus { supported: true, logged_in, detail }
}

/// 로그인 시작. 자식을 띄우고 stdout 을 그대로 채널로 흘린다 — URL 도 프롬프트도 에러도
/// CLI 가 쓴 문장 그대로 보여 준다(우리가 다시 쓰면 CLI 가 바뀔 때마다 어긋난다).
#[tauri::command]
pub async fn ai_auth_login(
    provider: Option<String>,
    cli_path: Option<String>,
    on_event: Channel<LoginEvent>,
) -> Result<(), AiError> {
    let (kind, program) = program_for(provider.as_deref(), cli_path.clone());
    let args = login_args(kind).ok_or_else(|| {
        AiError::new("AI_AUTH_UNSUPPORTED", "이 CLI 는 앱 안 로그인을 지원하지 않습니다.")
    })?;

    // 이전 시도가 남아 있으면 먼저 끝낸다 — 코드를 엉뚱한 프로세스에 넣지 않게.
    cancel_running();

    let mut child = Command::new(&program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr 도 같은 흐름에 섞는다 — 실패 사유가 어느 쪽으로 나오든 화면에 보여야 한다.
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AiError::detailed(
                    "AI_NOT_FOUND",
                    format!("AI CLI 를 찾을 수 없습니다: {program}"),
                    program.clone(),
                )
            } else {
                AiError::detailed("SPAWN_ERROR", e.to_string(), e.to_string())
            }
        })?;

    let pid = child.id().unwrap_or(0);
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    if let Ok(mut slot) = session().lock() {
        *slot = Some(LoginSession { pid, stdin });
    }

    tokio::spawn(async move {
        let _live = LiveGuard::new(Some(LOGIN_KEY), child.id());
        // 두 파이프를 각자 비운다 — 한쪽만 읽으면 다른 쪽이 가득 차 CLI 가 그 자리에서 멈춘다.
        let out_pump = tokio::spawn(pump(stdout, on_event.clone()));
        let err_pump = tokio::spawn(pump(stderr, on_event.clone()));
        let finish = async {
            let _ = out_pump.await;
            let _ = err_pump.await;
            let _ = child.wait().await;
        };
        // 모달을 닫고 잊어도 `auth login` 이 영영 남지 않게 상한을 둔다.
        if timeout(Duration::from_secs(LOGIN_TIMEOUT_SECS), finish).await.is_err() {
            let _ = child.start_kill();
        }
        take_session();
        let status = ai_auth_status(provider, cli_path).await;
        let _ = on_event.send(LoginEvent::Done { status });
    });

    Ok(())
}

/// 파이프 하나를 EOF 까지 비우며 조각을 그대로 흘린다.
async fn pump<R: tokio::io::AsyncRead + Unpin>(mut r: R, ch: Channel<LoginEvent>) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 2048];
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&buf[..n]).to_string();
        let _ = ch.send(LoginEvent::Output { text });
    }
}

/// 브라우저에서 받은 코드를 자식 stdin 으로 넘긴다. 어디에도 저장하지 않는다.
#[tauri::command]
pub async fn ai_auth_code(code: String) -> Result<(), AiError> {
    let mut sess = take_session()
        .ok_or_else(|| AiError::new("AI_AUTH_NO_SESSION", "진행 중인 로그인이 없습니다."))?;
    let line = format!("{}\n", code.trim());
    let r = sess.stdin.write_all(line.as_bytes()).await;
    // 코드가 틀리면 CLI 가 다시 묻는다 — 세션을 되돌려 놔야 두 번째 시도가 같은 프로세스로 간다.
    if let Ok(mut slot) = session().lock() {
        *slot = Some(sess);
    }
    r.map_err(|e| AiError::detailed("STDIN_ERROR", e.to_string(), e.to_string()))
}

/// 모달을 닫거나 다시 시작할 때 — 진행 중인 로그인을 끝낸다.
#[tauri::command]
pub fn ai_auth_cancel() {
    cancel_running();
}

fn cancel_running() {
    if let Some(sess) = take_session() {
        crate::ai::cancel(LOGIN_KEY);
        crate::ai::kill_pid(sess.pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // gemini 는 대화형 TUI 뿐이라 앱 안 로그인이 없다 — 지원 여부가 한 곳에서만 정해져야
    // 상태 조회와 로그인 시작이 서로 다른 말을 하지 않는다.
    #[test]
    fn only_clis_with_headless_login_are_supported() {
        assert!(login_args(ProviderKind::Claude).is_some());
        assert!(login_args(ProviderKind::Codex).is_some());
        assert!(login_args(ProviderKind::Gemini).is_none());
    }

    // 프론트(lib/auth.ts)가 읽는 모양 그대로 나가는지 — 태그·키가 어긋나면 타입 검사로는
    // 안 잡히고 로그인 창이 조용히 아무 것도 안 하게 된다.
    #[test]
    fn events_serialize_in_the_shape_the_ui_reads() {
        let out = serde_json::to_string(&LoginEvent::Output { text: "hi".into() }).unwrap();
        assert_eq!(out, r#"{"kind":"output","text":"hi"}"#);

        let done = serde_json::to_string(&LoginEvent::Done {
            status: AuthStatus { supported: true, logged_in: Some(true), detail: "claudeai".into() },
        })
        .unwrap();
        assert_eq!(
            done,
            r#"{"kind":"done","status":{"supported":true,"loggedIn":true,"detail":"claudeai"}}"#
        );

        // 확인 불가는 null 로 — false(만료)와 뜻이 다르다
        let unknown = serde_json::to_string(&AuthStatus::unsupported()).unwrap();
        assert_eq!(unknown, r#"{"supported":false,"loggedIn":null,"detail":""}"#);
    }

    // 설정에 경로가 없으면 PATH 의 기본 바이너리로 떨어져야 한다(온보딩 건너뛴 사용자)
    #[test]
    fn program_falls_back_to_the_default_binary() {
        let (_, p) = program_for(Some("claude"), None);
        assert_eq!(p, "claude");
        let (_, p) = program_for(Some("claude"), Some(String::new()));
        assert_eq!(p, "claude");
        let (_, p) = program_for(Some("codex"), Some("/opt/homebrew/bin/codex".into()));
        assert_eq!(p, "/opt/homebrew/bin/codex");
    }
}
