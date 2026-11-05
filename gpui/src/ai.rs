// AI 실행 다리 — amber-core 의 AI 함수(claude/codex CLI 를 띄운다)는 tokio 위에서 돈다. 화면(GPUI 실행기)과 섞지 않고
// 앱 전체에 tokio 런타임 하나를 두어 거기서 돌리고, 화면은 결과만 기다린다(오래 걸려도 창이 멈추지 않는다).
// 에러 문구는 여기서만 만든다 — Rust 는 코드만 돌려주고 사람이 읽을 문장은 UI 언어로 옮긴다(lib/errors.ts).

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use amber_core::ai::AiError;
use amber_gpui::i18n::{t, t_with};
use futures::channel::oneshot;
use gpui_kit::BackgroundExecutor;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("amber-ai")
            .build()
            .expect("AI 런타임")
    })
}

/// tokio 위에서 돌리고 결과를 받을 수신기 — GPUI 작업에서 [`wait`] 로 기다린다
pub fn spawn<T: Send + 'static>(
    fut: impl Future<Output = T> + Send + 'static,
) -> oneshot::Receiver<T> {
    let (tx, rx) = oneshot::channel();
    runtime().spawn(async move {
        let _ = tx.send(fut.await);
    });
    rx
}

/// 결과를 기다린다 — tokio 스레드가 화면 작업을 직접 깨우게 두지 않고, 화면 실행기의 타이머로 들여다본다.
/// 시험 실행기는 다른 스레드의 깨우기를 받지 않는다. 답은 초 단위라 50ms 간격이면 충분하다. None = 버려졌다
pub async fn wait<T>(mut rx: oneshot::Receiver<T>, ex: BackgroundExecutor) -> Option<T> {
    loop {
        match rx.try_recv() {
            Ok(Some(v)) => return Some(v),
            Ok(None) => ex.timer(Duration::from_millis(50)).await,
            Err(_) => return None,
        }
    }
}

/// 코드 → 사전 키(lib/errors.ts 의 CODE_KEY). 새 코드를 만들면 여기에 같이 더한다
const CODE_KEY: &[(&str, &str)] = &[
    ("AI_NOT_FOUND", "common.err.ai.notFound"),
    ("AI_AUTH", "common.err.ai.auth"),
    ("AI_AUTH_UNSUPPORTED", "common.err.ai.authUnsupported"),
    ("AI_AUTH_NO_SESSION", "common.err.ai.noLoginSession"),
    ("AI_RATE_LIMIT", "common.err.ai.rateLimit"),
    ("AI_TIMEOUT", "common.err.ai.timeout"),
    ("AI_BAD_ENVELOPE", "common.err.ai.badEnvelope"),
    ("AI_BAD_CONTRACT", "common.err.ai.badResult"),
    ("AI_ERROR", "common.err.ai.generic"),
    ("SPAWN_ERROR", "common.err.ai.spawn"),
    ("STDIN_ERROR", "common.err.ai.stdin"),
    ("WAIT_ERROR", "common.err.ai.wait"),
    ("DRAFT_DIR", "common.err.ai.draftDir"),
    ("EMPTY_TRANSCRIPT", "common.err.empty.transcript"),
    ("EMPTY_INSTRUCTION", "common.err.empty.instruction"),
    ("EMPTY_NOTE_BODY", "common.err.empty.noteBody"),
    ("EMPTY_QUESTION", "common.err.empty.question"),
    ("EMPTY_SELECTION", "common.err.empty.selection"),
    ("EMPTY_DDL", "common.err.empty.ddl"),
    ("NOTHING_TO_SUGGEST", "common.err.nothingToSuggest"),
    ("GH_NOT_FOUND", "common.err.gh.notFound"),
    ("GH_AUTH", "common.err.gh.auth"),
    ("GH_ACCOUNT", "common.err.gh.account"),
    ("GH_SSO", "common.err.gh.sso"),
    ("GH_ERROR", "common.err.gh.generic"),
    ("GH_WINDOW_TRUNCATED", "common.err.gh.windowTruncated"),
    ("REPORT_TIMEOUT", "common.err.report.timeout"),
    ("REPORT_NO_ACTIVITY", "common.err.report.noActivity"),
    ("BACKUP_NO_DEST", "common.err.backup.noDest"),
    ("BACKUP_INSIDE_APPDATA", "common.err.backup.insideAppData"),
    ("BACKUP_MKDIR", "common.err.backup.mkdir"),
    ("BACKUP_WRITE", "common.err.backup.write"),
    ("BACKUP_PATH", "common.err.backup.path"),
    ("TRASH_FORBIDDEN", "common.err.trash.forbidden"),
    ("TRASH_FAILED", "common.err.trash.failed"),
    ("DB_REFUSED", "common.err.db.refused"),
    ("DB_AUTH", "common.err.db.auth"),
    ("DB_TIMEOUT", "common.err.db.timeout"),
    ("DB_TLS", "common.err.db.tls"),
    ("DB_SCHEMA_DENIED", "common.err.db.schemaDenied"),
    ("DB_QUERY", "common.err.db.query"),
    ("DB_UNSUPPORTED", "common.err.db.unsupported"),
    ("DB_EMPTY_FIELD", "common.err.db.emptyField"),
    ("DB_EMPTY_PASSWORD", "common.err.db.emptyPassword"),
    ("DB_CREDENTIAL", "common.err.db.credential"),
    ("KEYCHAIN_DENIED", "common.err.keychain.denied"),
    ("KEYCHAIN_MISSING", "common.err.keychain.missing"),
];

/// errText — 코드가 있으면 번역, 미등록 코드는 Rust 의 폴백 문구(비면 코드)
pub fn err_text(e: &AiError) -> String {
    let Some((_, key)) = CODE_KEY.iter().find(|(c, _)| *c == e.code) else {
        return if e.message.is_empty() {
            e.code.clone()
        } else {
            e.message.clone()
        };
    };
    let text = t_with(key, &[("detail", e.detail.as_deref().unwrap_or(""))]);
    // detail 이 없는데 문구가 "… — " 로 끝나면 매달린 대시를 떼어 낸다
    let trimmed = text.trim_end();
    let trimmed = trimmed
        .strip_suffix('—')
        .or_else(|| trimmed.strip_suffix('-'))
        .map(str::trim_end)
        .unwrap_or(trimmed);
    if trimmed.is_empty() {
        t(key)
    } else {
        trimmed.to_string()
    }
}
