// 로컬 AI CLI 자동 감지 (온보딩/설정용).
// GUI 앱은 로그인 셸 PATH 를 상속받지 않으므로, 로그인 셸(-lc)로 `command -v` 를 실행해
// 사용자가 터미널에서 쓰는 그 바이너리를 찾는다. 각 후보는 --version 으로 동작까지 확인.

use serde::Serialize;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize)]
pub struct DetectedCli {
    /// 프로바이더 식별자: "claude" | "codex" | "gemini"
    pub id: String,
    /// 표시용 이름
    pub name: String,
    /// 로그인 셸 PATH 에서 해석된 절대경로
    pub path: String,
    /// `--version` 출력 (첫 줄)
    pub version: String,
}

const CANDIDATES: &[(&str, &str)] = &[
    ("claude", "Claude Code"),
    ("codex", "OpenAI Codex CLI"),
    ("gemini", "Gemini CLI"),
];

/// 로그인 셸로 바이너리 경로 해석 (zsh 기본, 실패 시 bash 폴백)
async fn resolve_path(bin: &str) -> Option<String> {
    for shell in ["/bin/zsh", "/bin/bash"] {
        let Ok(Ok(out)) = timeout(
            Duration::from_secs(8),
            Command::new(shell)
                .args(["-lc", &format!("command -v {bin}")])
                .output(),
        )
        .await
        else {
            continue;
        };
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() && p.starts_with('/') {
                return Some(p);
            }
        }
    }
    None
}

/// 버전 확인 — 실제로 실행 가능한지 검증을 겸한다
async fn probe_version(path: &str) -> Option<String> {
    let Ok(Ok(out)) = timeout(
        Duration::from_secs(8),
        Command::new(path).arg("--version").output(),
    )
    .await
    else {
        return None;
    };
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let first = s.lines().next().unwrap_or("").trim().to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

/// 설치된 AI CLI 목록 감지 (후보 동시 조회 — 로그인 셸 기동 지연이 곱해지지 않게)
#[tauri::command]
pub async fn detect_ai_clis() -> Vec<DetectedCli> {
    let handles: Vec<_> = CANDIDATES
        .iter()
        .map(|(id, name)| {
            let id = (*id).to_string();
            let name = (*name).to_string();
            tokio::spawn(async move {
                let path = resolve_path(&id).await?;
                let version = probe_version(&path).await?;
                Some(DetectedCli {
                    id,
                    name,
                    path,
                    version,
                })
            })
        })
        .collect();

    let mut out = Vec::new();
    for h in handles {
        if let Ok(Some(d)) = h.await {
            out.push(d);
        }
    }
    out
}
