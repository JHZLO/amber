// 로컬 AI CLI 자동 감지 (온보딩/설정용).
// GUI 앱은 로그인 셸 PATH 를 상속받지 않으므로, 로그인 셸(-lc)로 `command -v` 를 실행해
// 사용자가 터미널에서 쓰는 그 바이너리를 찾는다. 각 후보는 --version 으로 동작까지 확인.

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize)]
pub struct DetectedCli {
    /// 프로바이더 식별자: "claude" | "codex"
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
];

/// 로그인 셸로 바이너리 경로 해석 (zsh 기본, 실패 시 bash 폴백)
async fn resolve_path(bin: &str) -> Option<String> {
    for shell in ["/bin/zsh", "/bin/bash"] {
        let Ok(Ok(out)) = timeout(
            Duration::from_secs(8),
            Command::new(shell)
                .args(["-ilc", &format!("command -v {bin}")])
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        else {
            continue;
        };
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Some(p) = stdout
                .lines()
                .rev()
                .map(str::trim)
                .find(|l| l.starts_with('/'))
            {
                return Some(p.to_string());
            }
        }
    }
    None
}

/// 버전 확인 — 실제로 실행 가능한지 검증을 겸한다
async fn probe_version(path: &str) -> Option<String> {
    let Ok(Ok(out)) = timeout(
        Duration::from_secs(8),
        Command::new(path).arg("--version").kill_on_drop(true)
        .output(),
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

/// 설정 모델 목록의 한 항목. Codex 는 자기 카탈로그에서 읽고, Claude 는 카탈로그가 없어 프론트 큐레이션(config.ts)이다.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Codex 가 서버에서 받아 둔 모델 카탈로그(`$CODEX_HOME|~/.codex/models_cache.json`). codex TUI 의 /model 목록이
/// 쓰는 파일이라 CLI 가 아는 모델과 같다. 없거나 깨졌으면 빈 벡터 — 프론트가 큐레이션 목록으로 돌아간다.
#[tauri::command]
pub async fn codex_models() -> Vec<ModelInfo> {
    let dir = std::env::var("CODEX_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".codex")));
    let Some(dir) = dir else {
        return Vec::new();
    };
    match tokio::fs::read_to_string(dir.join("models_cache.json")).await {
        Ok(text) => parse_codex_models(&text),
        Err(_) => Vec::new(),
    }
}

/// `visibility == "list"` 인 것만, priority 오름차순(카탈로그가 정한 추천 순서). 숨김·내부용(auto review 등)은 뺀다.
pub(crate) fn parse_codex_models(json: &str) -> Vec<ModelInfo> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
        return Vec::new();
    };
    let mut out: Vec<(i64, ModelInfo)> = models
        .iter()
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?.trim();
            if slug.is_empty() || m.get("visibility").and_then(|x| x.as_str()) != Some("list") {
                return None;
            }
            let label = m.get("display_name").and_then(|x| x.as_str()).unwrap_or(slug).to_string();
            let description = m.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let prio = m.get("priority").and_then(|x| x.as_i64()).unwrap_or(i64::MAX);
            Some((prio, ModelInfo { id: slug.to_string(), label, description }))
        })
        .collect();
    out.sort_by_key(|(p, _)| *p);
    out.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 카탈로그의 숨김 항목(내부 리뷰 모델 등)은 빼고, 추천 순서(priority)대로 — 목록이 카탈로그 원본 순서를 따르면
    // 최신 모델이 맨 아래로 밀리거나 내부용 모델이 노출된다
    #[test]
    fn codex_catalog_keeps_listed_models_in_priority_order() {
        let json = r#"{"models":[
            {"slug":"gpt-old","display_name":"GPT-Old","description":"Proven","visibility":"list","priority":12},
            {"slug":"codex-auto-review","display_name":"Auto Review","visibility":"hide","priority":43},
            {"slug":"gpt-new","display_name":"GPT-New","description":"Most capable","visibility":"list","priority":1}
        ]}"#;
        let got = parse_codex_models(json);
        assert_eq!(got.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["gpt-new", "gpt-old"]);
        assert_eq!(got[0].label, "GPT-New");
        assert_eq!(got[0].description, "Most capable");
        assert!(parse_codex_models("not json").is_empty());
        assert!(parse_codex_models(r#"{"models":"nope"}"#).is_empty());
    }
}
