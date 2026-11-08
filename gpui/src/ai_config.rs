// lib/config.ts 를 옮긴 것 — AI 연결 설정(settings 표). 특정 벤더에 묶지 않는다: 온보딩/설정에서 연결한 제공자
// (claude/codex)를 ai_provider 로, 경로와 모델은 제공자별 키로 둔다. gemini 는 v0.20.10 에서 지원을 끝냈다 —
// 저장돼 있던 "gemini" 는 걸러져 미연결로 돌아간다. 모델이 비면 CLI 기본(--model 을 넘기지 않는다).

use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::concepts::{get_setting, set_setting};
use crate::db::{Db, Result};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// None = 미연결(AI 기능 꺼짐)
    pub provider: Option<String>,
    pub onboarded: bool,
    pub cli_path: String,
    pub model: String,
    /// 'auto' = 화면 언어를 따른다
    pub ai_lang: String,
}

fn is_provider(v: &str) -> bool {
    v == "claude" || v == "codex"
}

fn path_key(p: &str) -> String {
    format!("ai_path_{p}")
}

fn model_key(p: &str) -> String {
    format!("ai_model_{p}")
}

fn load_ai_lang(db: &Db) -> Result<String> {
    let saved = get_setting(db, "ai_lang")?;
    Ok(match saved.as_deref() {
        Some(l @ ("auto" | "ko" | "en")) => l.to_string(),
        _ => "auto".to_string(),
    })
}

pub fn load_config(db: &Db) -> Result<AppConfig> {
    let mut provider = get_setting(db, "ai_provider")?.filter(|p| is_provider(p));
    let mut onboarded = get_setting(db, "ai_onboarded")?.as_deref() == Some("1");
    // 옛 버전 이전: ai_provider 가 없지만 claude 설정이 있으면 claude 로 본다
    if provider.is_none()
        && !onboarded
        && let Some(legacy) = get_setting(db, "claude_path")?.filter(|p| !p.is_empty())
    {
        provider = Some("claude".into());
        onboarded = true;
        set_setting(db, "ai_provider", "claude")?;
        set_setting(db, "ai_onboarded", "1")?;
        set_setting(db, &path_key("claude"), &legacy)?;
        if let Some(m) = get_setting(db, "claude_model")?.filter(|m| !m.is_empty()) {
            set_setting(db, &model_key("claude"), &m)?;
        }
    }
    let ai_lang = load_ai_lang(db)?;
    let Some(p) = provider else {
        return Ok(AppConfig {
            provider: None,
            onboarded,
            cli_path: String::new(),
            model: String::new(),
            ai_lang,
        });
    };
    Ok(AppConfig {
        cli_path: get_setting(db, &path_key(&p))?.unwrap_or_default(),
        model: get_setting(db, &model_key(&p))?.unwrap_or_default(),
        provider: Some(p),
        onboarded,
        ai_lang,
    })
}

/// 제공자 연결(온보딩/설정 공용). 경로와 모델을 제공자별 키에
pub fn connect_provider(
    db: &Db,
    provider: &str,
    cli_path: &str,
    model: Option<&str>,
) -> Result<AppConfig> {
    set_setting(db, "ai_provider", provider)?;
    set_setting(db, "ai_onboarded", "1")?;
    set_setting(db, &path_key(provider), crate::vault::js_trim(cli_path))?;
    if let Some(m) = model {
        set_setting(db, &model_key(provider), m)?;
    }
    load_config(db)
}

/// AI 없이 쓰기(온보딩 건너뛰기)
pub fn skip_ai_onboarding(db: &Db) -> Result<AppConfig> {
    set_setting(db, "ai_onboarded", "1")?;
    load_config(db)
}

/// 설정 화면 저장 — 언어는 제공자와 무관하게 먼저(미연결에서도 고를 수 있다)
pub fn save_config(db: &Db, c: &AppConfig) -> Result<()> {
    set_setting(db, "ai_lang", &c.ai_lang)?;
    let Some(p) = c.provider.as_deref() else {
        return Ok(());
    };
    set_setting(db, "ai_provider", p)?;
    set_setting(db, &path_key(p), crate::vault::js_trim(&c.cli_path))?;
    set_setting(db, &model_key(p), &c.model)?;
    Ok(())
}

/// AI 응답 언어 — 'auto' 면 화면 언어
pub fn output_lang(c: &AppConfig) -> String {
    if c.ai_lang == "auto" {
        if crate::i18n::is_ko() { "ko" } else { "en" }.to_string()
    } else {
        c.ai_lang.clone()
    }
}

/// 하네스가 재는 연산
pub fn call(db: &Db, op: &str, args: &[Json]) -> Option<std::result::Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let to = |r: Result<AppConfig>| r.map(|c| json!(c)).map_err(|e| e.to_string());
    Some(match op {
        "config.loadConfig" => to(load_config(db)),
        "config.connectProvider" => to(connect_provider(
            db,
            s(0),
            s(1),
            args.get(2).and_then(Json::as_str),
        )),
        "config.skipAiOnboarding" => to(skip_ai_onboarding(db)),
        "config.saveConfig" => {
            let c = &args[0];
            let cfg = AppConfig {
                provider: c["provider"].as_str().map(str::to_string),
                onboarded: c["onboarded"].as_bool().unwrap_or(false),
                cli_path: c["cliPath"].as_str().unwrap_or_default().to_string(),
                model: c["model"].as_str().unwrap_or_default().to_string(),
                ai_lang: c["aiLang"].as_str().unwrap_or_default().to_string(),
            };
            save_config(db, &cfg)
                .map(|_| Json::Null)
                .map_err(|e| e.to_string())
        }
        _ => return None,
    })
}
