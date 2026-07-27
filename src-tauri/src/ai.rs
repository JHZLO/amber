// AI 프로바이더 브리지 (claude/codex/gemini) — headless CLI 로 노트 생성/보강/질문.
// 붙여넣은 AI Q&A 원문 → 로컬 CLI (claude 는 `claude -p --output-format json`) → 요약+상세 노트(JSON).
// 봉투(envelope) 안의 .result 문자열에 우리 계약 JSON 이 또 들어있어 "이중 파싱"이 필요하다 (PRD §6).
// stdin 으로 원문을 넘기고 EOF 를 확실히 닫기 위해 tokio::process 를 직접 사용한다.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_MODEL: &str = "claude-opus-4-8";
// 상세 노트 생성(특히 sonnet 다중 턴 + mermaid)이 오래 걸려 넉넉하게 5분.
const DEFAULT_TIMEOUT_SECS: u64 = 300;
const MIN_INPUT_CHARS: usize = 20;
// `--version` 헬스체크 상한 — detect.rs 의 probe_version 과 같은 값(설정 모달 스피너가 멈추지 않게).
const HEALTH_TIMEOUT_SECS: u64 = 8;
// 스트림이 끝난 뒤 프로세스 종료를 기다리는 상한. stdio MCP 서버 teardown 이 늦어질 수 있는데,
// 출력은 이미 다 받은 뒤라 더 기다릴 이유가 없다.
const POST_STREAM_WAIT_SECS: u64 = 10;

// 시스템 프롬프트는 src-tauri/context/*.md 에 영어로 두고 컴파일 시 그대로 임베드한다
// (include_str!). 파일 = 모델에 전달되는 프롬프트 본문 그 자체이며, 고치면 재빌드가 필요하다.
// 상세는 context/README.md 참고. 결과물은 각 프롬프트의 "주 언어(대개 한국어)" 규칙으로 한글 유지.

// 개념 카드 생성 + 선택 영역 승격 공용. 출력: raw JSON(Contract).
const SYSTEM_PROMPT: &str = include_str!("../context/concept-generate.md");

// 기존 개념 노트 보강. 출력: raw JSON(Contract).
const AUGMENT_SYSTEM_PROMPT: &str = include_str!("../context/concept-augment.md");

// 노트는 detail_markdown 만 쓰므로 JSON 계약을 강요하지 않는다.
// 큰 마크다운을 JSON 문자열에 담게 하면(특히 sonnet) 이스케이프/전체 코드펜스 래핑으로 이중 파싱이
// 간헐적으로 깨진다 → "raw 마크다운 그 자체"만 받고 봉투 .result 를 그대로 본문으로 쓴다(CLI 가 이스케이프 담당).
const NOTE_SYSTEM_PROMPT: &str = include_str!("../context/note-compose.md");

// 필기노트 인라인 질문(노션 댓글식): 드래그한 문장 + 질문 → 짧은 답변.
// 노트 본문을 불리지 않는 별도 Q&A 라 "간결함"을 프롬프트로 강제한다.
const ASK_SYSTEM_PROMPT: &str = include_str!("../context/note-ask.md");

// 다이어그램 탭: 스키마 DDL → ERD mermaid 소스. 노트와 같은 이유로 JSON 계약 없이 raw 텍스트.
const ERD_SYSTEM_PROMPT: &str = include_str!("../context/diagram-erd.md");

// ---- 프로바이더 추상화 ----
// AI 를 특정 벤더에 묶지 않는다. claude 는 풍부한 경로(JSON 봉투 + 스트리밍)를 쓰고,
// codex/gemini 는 "stdin 프롬프트 → 최종 텍스트" 공통 경로를 쓴다.
// (codex: `exec -` 가 stdin 을 프롬프트로 읽고 최종 메시지만 stdout, 진행 로그는 stderr.
//  gemini: non-TTY stdin 파이프 + --output-format json 의 .response 가 최종 텍스트.)

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProviderKind {
    Claude,
    Codex,
    Gemini,
}

pub(crate) fn provider_kind(p: Option<&str>) -> ProviderKind {
    match p {
        Some("codex") => ProviderKind::Codex,
        Some("gemini") => ProviderKind::Gemini,
        _ => ProviderKind::Claude,
    }
}

pub(crate) fn default_binary(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Claude => "claude",
        ProviderKind::Codex => "codex",
        ProviderKind::Gemini => "gemini",
    }
}

/// 모델 결정: claude 는 기본 모델 폴백, codex/gemini 는 빈 값 = CLI 기본 모델 사용(-m 미전달)
pub(crate) fn resolve_model(kind: ProviderKind, model: Option<String>) -> String {
    let m = model.filter(|m| !m.is_empty());
    match kind {
        ProviderKind::Claude => m.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        _ => m.unwrap_or_default(),
    }
}

/// 프로바이더 공용 실행: 시스템 프롬프트 + 입력 → 최종 텍스트(.result 상당) + 메타
pub(crate) async fn run_provider_text(
    kind: ProviderKind,
    program: String,
    model: String,
    dur: Duration,
    system_prompt: &str,
    input: String,
) -> Result<(String, MetaOut), AiError> {
    match kind {
        ProviderKind::Claude => {
            spawn_claude_result(program, model, dur, system_prompt, input).await
        }
        _ => spawn_simple_cli_result(kind, program, model, dur, system_prompt, input).await,
    }
}

/// codex/gemini 공용: 시스템 프롬프트를 프롬프트 상단에 합쳐(전용 플래그 없음) stdin 으로 전달.
async fn spawn_simple_cli_result(
    kind: ProviderKind,
    program: String,
    model: String,
    dur: Duration,
    system_prompt: &str,
    input: String,
) -> Result<(String, MetaOut), AiError> {
    let combined = format!("[지시사항 — 반드시 그대로 따를 것]\n{system_prompt}\n\n{input}");
    let started = std::time::Instant::now();

    let mut cmd = Command::new(&program);
    match kind {
        ProviderKind::Codex => {
            // `-` = stdin 프롬프트. --ephemeral: 세션 파일 미저장, --skip-git-repo-check: repo 밖 실행 허용.
            // exec 기본 샌드박스는 read-only 라 순수 텍스트 변환에 안전.
            cmd.args(["exec", "-", "--ephemeral", "--skip-git-repo-check"]);
            if !model.is_empty() {
                cmd.args(["-m", &model]);
            }
        }
        ProviderKind::Gemini => {
            cmd.args(["--output-format", "json"]);
            if !model.is_empty() {
                cmd.args(["-m", &model]);
            }
        }
        ProviderKind::Claude => unreachable!("claude 는 spawn_claude_result 경로"),
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AiError::new(
                    "AI_NOT_FOUND",
                    format!("AI CLI 를 찾을 수 없습니다: {program}"),
                )
            } else {
                AiError::new("SPAWN_ERROR", e.to_string())
            }
        })?;

    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(combined.as_bytes())
            .await
            .map_err(|e| AiError::new("STDIN_ERROR", e.to_string()))?;
        let _ = stdin.shutdown().await;
    }

    let output = match timeout(dur, child.wait_with_output()).await {
        Err(_) => {
            return Err(AiError::new(
                "AI_TIMEOUT",
                format!("{}초 안에 응답이 없습니다.", dur.as_secs()),
            ))
        }
        Ok(r) => r.map_err(|e| AiError::new("WAIT_ERROR", e.to_string()))?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        let (code, msg) = if stderr.contains("auth")
            || stderr.contains("login")
            || stderr.contains("unauthorized")
            || stderr.contains("credential")
        {
            (
                "AI_AUTH",
                "AI CLI 인증이 필요합니다. 터미널에서 로그인 후 다시 시도하세요.",
            )
        } else if stderr.contains("rate") || stderr.contains("quota") || stderr.contains("limit") {
            (
                "AI_RATE_LIMIT",
                "사용량 한도에 도달했습니다. 잠시 후 다시 시도하세요.",
            )
        } else {
            ("AI_ERROR", "AI CLI 실행이 실패했습니다.")
        };
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::new(code, format!("{msg}\n{}", detail.trim())));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = match kind {
        // gemini: JSON 봉투의 .response 가 최종 텍스트 (파싱 실패 시 raw 폴백)
        ProviderKind::Gemini => serde_json::from_str::<serde_json::Value>(stdout.trim())
            .ok()
            .and_then(|v| v.get("response").and_then(|r| r.as_str()).map(String::from))
            .unwrap_or_else(|| stdout.trim().to_string()),
        // codex: 최종 메시지만 stdout 에 나온다
        _ => stdout.trim().to_string(),
    };
    if text.is_empty() {
        return Err(AiError::new("AI_ERROR", "빈 응답입니다."));
    }

    let meta = MetaOut {
        model,
        session_id: None,
        cost_usd: None,
        input_tokens: None,
        output_tokens: None,
        duration_ms: Some(started.elapsed().as_millis() as i64),
    };
    Ok((text, meta))
}

/// 프론트에 돌려줄 결과 = 노트 + 호출 메타(로깅용)
#[derive(Debug, Serialize)]
pub struct GenerateResult {
    pub note: NoteOut,
    pub meta: MetaOut,
}

#[derive(Debug, Serialize)]
pub struct NoteOut {
    pub title: String,
    pub summary: String,
    pub detail_markdown: String,
    pub tags: Vec<String>,
    pub confidence_suggestion: u8,
    pub source_excerpt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetaOut {
    pub model: String,
    pub session_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub duration_ms: Option<i64>,
}

/// 필기노트 작성 결과 = 마크다운 본문 + 메타 (JSON 계약 없이 raw 마크다운)
#[derive(Debug, Serialize)]
pub struct NoteComposeResult {
    pub markdown: String,
    pub meta: MetaOut,
}

/// 인라인 질문 답변 결과 = 짧은 답변 텍스트 + 메타
#[derive(Debug, Serialize)]
pub struct NoteAskResult {
    pub answer: String,
    pub meta: MetaOut,
}

/// DDL → ERD 변환 결과 = mermaid 소스 + 메타
#[derive(Debug, Serialize)]
pub struct ErdResult {
    pub mermaid: String,
    pub meta: MetaOut,
}

/// 프론트가 케이스별로 분기할 수 있게 code + message 로 반환
#[derive(Debug, Serialize)]
pub struct AiError {
    pub code: String,
    pub message: String,
}

impl AiError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

// ---- 봉투 / 계약 파싱 타입 ----

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    subtype: Option<String>,
    result: Option<String>,
    session_id: Option<String>,
    total_cost_usd: Option<f64>,
    duration_ms: Option<i64>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
}

#[derive(Deserialize)]
struct Contract {
    title: String,
    summary: String,
    detail_markdown: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_conf")]
    confidence_suggestion: u8,
    #[serde(default)]
    source_excerpt: Option<String>,
}

fn default_conf() -> u8 {
    1
}

/// 최외곽 코드펜스만 제거(detail_markdown 안의 예시 코드블록은 건드리지 않도록 전체 문자열 기준).
pub(crate) fn strip_outer_fence(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // 첫 줄(언어 태그 가능)을 버리고, 끝의 ``` 제거
        if let Some(nl) = rest.find('\n') {
            let body = &rest[nl + 1..];
            if let Some(end) = body.rfind("```") {
                return body[..end].trim();
            }
        }
    }
    t
}

#[tauri::command]
pub async fn ai_generate(
    transcript: String,
    instruction: Option<String>,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<GenerateResult, AiError> {
    if transcript.trim().chars().count() < MIN_INPUT_CHARS {
        return Err(AiError::new(
            "EMPTY_INPUT",
            "입력이 너무 짧습니다. 대화 원문을 붙여넣어 주세요.",
        ));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    // 사용자 지시문이 있으면 원문 앞에 섹션으로 덧붙인다 (시스템 프롬프트가 이를 최우선 반영).
    let input = match instruction.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(instr) => format!("[사용자 추가 지시]\n{instr}\n\n[대화 원문]\n{transcript}"),
        None => transcript,
    };

    run_concept_note(kind, program, model, dur, SYSTEM_PROMPT, input).await
}

/// 이미 정리된 노트 + 사용자 프롬프트 → 보강된 노트(JSON).
/// 원문 요약(ai_generate)과 파이프라인은 같지만, 입력 구성과 시스템 프롬프트만 다르다.
#[tauri::command]
pub async fn ai_augment(
    title: String,
    summary: String,
    tags: Vec<String>,
    markdown: String,
    instruction: String,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<GenerateResult, AiError> {
    let instr = instruction.trim();
    if instr.is_empty() {
        return Err(AiError::new(
            "EMPTY_INPUT",
            "보강 지시를 입력해 주세요.",
        ));
    }
    if markdown.trim().is_empty() {
        return Err(AiError::new(
            "EMPTY_INPUT",
            "보강할 노트 본문이 비어 있습니다.",
        ));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let tags_line = tags.join(", ");
    let input = format!(
        "[보강 요청]\n{instr}\n\n[현재 노트]\n제목: {title}\n요약: {summary}\n태그: {tags_line}\n\n[현재 상세 노트 (Markdown)]\n{markdown}"
    );

    run_concept_note(kind, program, model, dur, AUGMENT_SYSTEM_PROMPT, input).await
}

/// 필기노트 작성/보강: 자유 형식 마크다운 + 지시 → 완성본(raw 마크다운).
/// 개념 정리(ai_generate)와 달리 JSON 계약을 파싱하지 않는다 — 봉투 .result 를 그대로 본문으로.
/// 빈 본문 허용(그 경우 요청 주제로 처음부터 작성).
#[tauri::command]
pub async fn ai_note_compose(
    title: String,
    markdown: String,
    instruction: String,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<NoteComposeResult, AiError> {
    let instr = instruction.trim();
    if instr.is_empty() {
        return Err(AiError::new("EMPTY_INPUT", "작성 지시를 입력해 주세요."));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let body = markdown.trim();
    let body = if body.is_empty() { "(비어 있음)" } else { body };
    let input = format!(
        "[작성 요청]\n{instr}\n\n[현재 노트]\n제목: {title}\n\n[현재 본문 (Markdown)]\n{body}"
    );

    let (result_str, meta) =
        run_provider_text(kind, program, model, dur, NOTE_SYSTEM_PROMPT, input).await?;

    // 전체를 감싼 코드펜스만 벗기고(본문 내부 코드블록은 보존) 그대로 마크다운 본문으로 사용
    let md = strip_outer_fence(&result_str).trim().to_string();
    if md.is_empty() {
        return Err(AiError::new(
            "AI_BAD_CONTRACT",
            "생성된 노트 내용이 비어 있습니다. 다시 시도해 주세요.",
        ));
    }

    Ok(NoteComposeResult { markdown: md, meta })
}

/// 필기노트 작성/보강 (스트리밍). 생성 텍스트 델타를 on_delta 채널로 실시간 전송하고,
/// 최종 결과는 stream-json 의 마지막 `result` 봉투(.result)에서 확정한다(신뢰 소스).
#[tauri::command]
pub async fn ai_note_compose_stream(
    title: String,
    markdown: String,
    instruction: String,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
    on_delta: Channel<String>,
) -> Result<NoteComposeResult, AiError> {
    let instr = instruction.trim();
    if instr.is_empty() {
        return Err(AiError::new("EMPTY_INPUT", "작성 지시를 입력해 주세요."));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let body = markdown.trim();
    let body = if body.is_empty() { "(비어 있음)" } else { body };
    let input = format!(
        "[작성 요청]\n{instr}\n\n[현재 노트]\n제목: {title}\n\n[현재 본문 (Markdown)]\n{body}"
    );

    let (result_str, meta) = if kind == ProviderKind::Claude {
        stream_claude_result(program, model, dur, NOTE_SYSTEM_PROMPT, input, &[], &on_delta).await?
    } else {
        // codex/gemini 는 스트리밍 미지원 경로 — 완료 후 전체 텍스트를 한 번에 전송
        let r = run_provider_text(kind, program, model, dur, NOTE_SYSTEM_PROMPT, input).await?;
        let _ = on_delta.send(r.0.clone());
        r
    };

    let md = strip_outer_fence(&result_str).trim().to_string();
    if md.is_empty() {
        return Err(AiError::new(
            "AI_BAD_CONTRACT",
            "생성된 노트 내용이 비어 있습니다. 다시 시도해 주세요.",
        ));
    }

    Ok(NoteComposeResult { markdown: md, meta })
}

/// 후속 질문에 실어 보내는 이전 문답 한 쌍 (프론트 사이드카의 스레드에서 옴)
#[derive(Debug, Deserialize)]
pub struct AskExchange {
    pub question: String,
    pub answer: String,
}

/// 필기노트 인라인 질문: 선택 문장 + 질문 (+ 이전 문답) + 노트 문맥 → 짧은 답변(raw 텍스트).
#[tauri::command]
pub async fn ai_note_ask(
    selection: String,
    question: String,
    note_markdown: String,
    history: Option<Vec<AskExchange>>,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<NoteAskResult, AiError> {
    let q = question.trim();
    if q.is_empty() {
        return Err(AiError::new("EMPTY_INPUT", "질문을 입력해 주세요."));
    }
    let sel = selection.trim();
    if sel.is_empty() {
        return Err(AiError::new("EMPTY_INPUT", "선택한 문장이 비어 있습니다."));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let note = note_markdown.trim();
    let note = if note.is_empty() { "(비어 있음)" } else { note };
    let mut input = format!("[질문]\n{q}\n\n[선택한 부분]\n{sel}\n\n");
    if let Some(hist) = history.as_deref().filter(|h| !h.is_empty()) {
        input.push_str("[이전 문답]\n");
        for turn in hist {
            input.push_str(&format!(
                "Q: {}\nA: {}\n\n",
                turn.question.trim(),
                turn.answer.trim()
            ));
        }
    }
    input.push_str(&format!("[노트 전체 (Markdown)]\n{note}"));

    let (result_str, meta) =
        run_provider_text(kind, program, model, dur, ASK_SYSTEM_PROMPT, input).await?;

    let answer = strip_outer_fence(&result_str).trim().to_string();
    if answer.is_empty() {
        return Err(AiError::new(
            "AI_BAD_CONTRACT",
            "답변이 비어 있습니다. 다시 시도해 주세요.",
        ));
    }

    Ok(NoteAskResult { answer, meta })
}

/// 다이어그램 탭: 스키마 DDL → ERD mermaid 소스 (스트리밍).
/// 노트 작성과 같은 raw 텍스트 경로 — 큰 mermaid 소스를 JSON 문자열에 담는 이중 파싱을 피한다.
/// current(에디터에 열려 있는 기존 소스)를 주면 그 문법·구성을 이어받아 확장한다.
#[tauri::command]
pub async fn ai_erd_generate_stream(
    ddl: String,
    instruction: Option<String>,
    current: Option<String>,
    model: Option<String>,
    cli_path: Option<String>,
    provider: Option<String>,
    timeout_secs: Option<u64>,
    on_delta: Channel<String>,
) -> Result<ErdResult, AiError> {
    if ddl.trim().chars().count() < MIN_INPUT_CHARS {
        return Err(AiError::new(
            "EMPTY_INPUT",
            "스키마 DDL 을 붙여넣어 주세요.",
        ));
    }

    let kind = provider_kind(provider.as_deref());
    let program =
        cli_path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_binary(kind).to_string());
    let model = resolve_model(kind, model);
    let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let mut input = String::new();
    if let Some(instr) = instruction.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        input.push_str(&format!("[추가 지시]\n{instr}\n\n"));
    }
    if let Some(cur) = current.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        input.push_str(&format!("[현재 다이어그램 (Mermaid)]\n{cur}\n\n"));
    }
    input.push_str(&format!("[스키마 DDL]\n{}", ddl.trim()));

    let (result_str, meta) = if kind == ProviderKind::Claude {
        stream_claude_result(program, model, dur, ERD_SYSTEM_PROMPT, input, &[], &on_delta).await?
    } else {
        // codex/gemini 는 스트리밍 미지원 경로 — 완료 후 전체 텍스트를 한 번에 전송
        let r = run_provider_text(kind, program, model, dur, ERD_SYSTEM_PROMPT, input).await?;
        let _ = on_delta.send(r.0.clone());
        r
    };

    // 전체를 감싼 ```mermaid 펜스만 벗긴다 (mermaid 소스 안에는 코드펜스가 없다)
    let mermaid = strip_outer_fence(&result_str).trim().to_string();
    if mermaid.is_empty() {
        return Err(AiError::new(
            "AI_BAD_CONTRACT",
            "생성된 다이어그램이 비어 있습니다. 다시 시도해 주세요.",
        ));
    }

    Ok(ErdResult { mermaid, meta })
}

/// stream-json 실행: 줄 단위로 읽어 text_delta 를 on_delta 로 흘리고,
/// 마지막 `result` 봉투에서 최종 .result + 메타를 확정해 돌려준다.
pub(crate) async fn stream_claude_result(
    program: String,
    model: String,
    dur: Duration,
    system_prompt: &str,
    input: String,
    // 추가 CLI 인자(예: MCP --allowedTools/--mcp-config). 노트 경로는 빈 슬라이스를 넘긴다.
    extra_args: &[String],
    on_delta: &Channel<String>,
) -> Result<(String, MetaOut), AiError> {
    let mut child = Command::new(&program)
        .arg("-p")
        .args(["--output-format", "stream-json"])
        .arg("--include-partial-messages")
        .arg("--verbose")
        .args(["--model", &model])
        .args(["--append-system-prompt", system_prompt])
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AiError::new(
                    "AI_NOT_FOUND",
                    format!("claude CLI 를 찾을 수 없습니다: {program}"),
                )
            } else {
                AiError::new("SPAWN_ERROR", e.to_string())
            }
        })?;

    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| AiError::new("STDIN_ERROR", e.to_string()))?;
        let _ = stdin.shutdown().await;
    }

    // stderr 는 별도 태스크로 동시에 비운다(파이프 가득 참으로 인한 교착 방지).
    let stderr = child.stderr.take().expect("stderr piped");
    let mut stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut r = BufReader::new(stderr);
        let _ = r.read_to_string(&mut buf).await;
        buf
    });

    let stdout = child.stdout.take().expect("stdout piped");
    let mut lines = BufReader::new(stdout).lines();
    let mut envelope: Option<Envelope> = None;

    let read_loop = async {
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| AiError::new("WAIT_ERROR", e.to_string()))?
        {
            if line.trim().is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue, // JSON 아닌 줄(경고 등) 무시
            };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("stream_event") => {
                    let ev = v.get("event");
                    let is_delta = ev.and_then(|e| e.get("type")).and_then(|t| t.as_str())
                        == Some("content_block_delta");
                    if is_delta {
                        if let Some(text) = ev
                            .and_then(|e| e.get("delta"))
                            .filter(|d| {
                                d.get("type").and_then(|t| t.as_str()) == Some("text_delta")
                            })
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            let _ = on_delta.send(text.to_string());
                        }
                    }
                }
                Some("result") => {
                    // 최종 봉투(신뢰 소스). 뒤에 더 오지 않지만 방어적으로 계속 읽는다.
                    envelope = serde_json::from_str::<Envelope>(&line).ok();
                }
                _ => {}
            }
        }
        Ok::<(), AiError>(())
    };

    match timeout(dur, read_loop).await {
        Err(_) => {
            let _ = child.start_kill();
            return Err(AiError::new(
                "AI_TIMEOUT",
                format!("{}초 안에 응답이 없습니다.", dur.as_secs()),
            ));
        }
        Ok(r) => r?,
    }

    // 결과 봉투까지 다 받아도 child 종료는 별개다 — 여기서 무한정 기다리면 호출부(리포트 생성 등)가
    // 영영 반환하지 못하므로 상한을 두고 안 끝나면 죽인다.
    let post = Duration::from_secs(POST_STREAM_WAIT_SECS);
    if timeout(post, child.wait()).await.is_err() {
        let _ = child.start_kill();
    }
    // stderr 파이프는 손자 프로세스(MCP 서버)가 붙들고 있을 수 있다. 진단용이라 못 받으면 포기.
    let errbuf = match timeout(post, &mut stderr_task).await {
        Ok(r) => r.unwrap_or_default(),
        Err(_) => {
            stderr_task.abort();
            String::new()
        }
    };

    let envelope = match envelope {
        Some(e) => e,
        None => {
            let low = errbuf.to_lowercase();
            let (code, msg) = if low.contains("auth")
                || low.contains("login")
                || low.contains("unauthorized")
            {
                (
                    "AI_AUTH",
                    "claude 인증이 필요합니다. 터미널에서 `claude` 로그인 후 다시 시도하세요.",
                )
            } else if low.contains("rate") || low.contains("quota") || low.contains("limit") {
                (
                    "AI_RATE_LIMIT",
                    "사용량 한도에 도달했습니다. 잠시 후 다시 시도하세요.",
                )
            } else {
                ("AI_BAD_ENVELOPE", "스트림에서 결과를 받지 못했습니다.")
            };
            return Err(AiError::new(code, format!("{msg}\n{}", errbuf.trim())));
        }
    };

    if envelope.is_error || envelope.subtype.as_deref() != Some("success") {
        return Err(AiError::new("AI_ERROR", "claude 가 오류를 반환했습니다."));
    }
    let result_str = envelope
        .result
        .clone()
        .ok_or_else(|| AiError::new("AI_ERROR", "빈 응답입니다."))?;

    let usage = envelope.usage;
    let meta = MetaOut {
        model,
        session_id: envelope.session_id,
        cost_usd: envelope.total_cost_usd,
        input_tokens: usage.as_ref().and_then(|u| u.input_tokens),
        output_tokens: usage.as_ref().and_then(|u| u.output_tokens),
        duration_ms: envelope.duration_ms,
    };
    Ok((result_str, meta))
}

/// stdout 이 훅/경고 텍스트로 오염됐을 때를 대비해 최외곽 JSON 객체만 도려내 재시도한다.
fn parse_envelope(stdout: &str) -> Result<Envelope, AiError> {
    let trimmed = stdout.trim();
    if let Ok(e) = serde_json::from_str::<Envelope>(trimmed) {
        return Ok(e);
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            if let Ok(e) = serde_json::from_str::<Envelope>(&trimmed[start..=end]) {
                return Ok(e);
            }
        }
    }
    Err(AiError::new(
        "AI_BAD_ENVELOPE",
        "CLI 응답(JSON 봉투) 해석 실패. `claude --version` 확인이 필요할 수 있습니다.",
    ))
}

/// claude headless 공용 실행: spawn → stdin 주입 → 봉투 파싱까지.
/// 봉투 .result(문자열)와 메타를 돌려주고, 그 이후 해석(계약 파싱 / 마크다운)은 호출부가 정한다.
async fn spawn_claude_result(
    program: String,
    model: String,
    dur: Duration,
    system_prompt: &str,
    input: String,
) -> Result<(String, MetaOut), AiError> {
    let mut child = Command::new(&program)
        .arg("-p")
        .args(["--output-format", "json"])
        .args(["--model", &model])
        .args(["--append-system-prompt", system_prompt])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AiError::new(
                    "AI_NOT_FOUND",
                    format!("claude CLI 를 찾을 수 없습니다: {program}"),
                )
            } else {
                AiError::new("SPAWN_ERROR", e.to_string())
            }
        })?;

    // stdin 으로 원문 전달 후 닫기(EOF)
    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| AiError::new("STDIN_ERROR", e.to_string()))?;
        let _ = stdin.shutdown().await;
    }

    let output = match timeout(dur, child.wait_with_output()).await {
        Err(_) => {
            return Err(AiError::new(
                "AI_TIMEOUT",
                format!("{}초 안에 응답이 없습니다.", dur.as_secs()),
            ))
        }
        Ok(r) => r.map_err(|e| AiError::new("WAIT_ERROR", e.to_string()))?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        let (code, msg) = if stderr.contains("auth")
            || stderr.contains("login")
            || stderr.contains("unauthorized")
        {
            (
                "AI_AUTH",
                "claude 인증이 필요합니다. 터미널에서 `claude` 로그인 후 다시 시도하세요.",
            )
        } else if stderr.contains("rate") || stderr.contains("quota") || stderr.contains("limit") {
            (
                "AI_RATE_LIMIT",
                "사용량 한도에 도달했습니다. 잠시 후 다시 시도하세요.",
            )
        } else {
            ("AI_ERROR", "claude 실행이 실패했습니다.")
        };
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::new(code, format!("{msg}\n{}", detail.trim())));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope = parse_envelope(&stdout)?;

    if envelope.is_error || envelope.subtype.as_deref() != Some("success") {
        return Err(AiError::new("AI_ERROR", "claude 가 오류를 반환했습니다."));
    }

    let result_str = envelope
        .result
        .ok_or_else(|| AiError::new("AI_ERROR", "빈 응답입니다."))?;

    let usage = envelope.usage;
    let meta = MetaOut {
        model,
        session_id: envelope.session_id,
        cost_usd: envelope.total_cost_usd,
        input_tokens: usage.as_ref().and_then(|u| u.input_tokens),
        output_tokens: usage.as_ref().and_then(|u| u.output_tokens),
        duration_ms: envelope.duration_ms,
    };
    Ok((result_str, meta))
}

/// 개념 정리 파이프라인: 공용 실행 후 .result 를 우리 JSON 계약(Contract)으로 이중 파싱·정규화.
async fn run_concept_note(
    kind: ProviderKind,
    program: String,
    model: String,
    dur: Duration,
    system_prompt: &str,
    input: String,
) -> Result<GenerateResult, AiError> {
    let (result_str, meta) =
        run_provider_text(kind, program, model, dur, system_prompt, input).await?;

    let contract: Contract = serde_json::from_str(strip_outer_fence(&result_str)).map_err(|e| {
        AiError::new(
            "AI_BAD_CONTRACT",
            format!("정리 결과(JSON) 파싱 실패: {e}"),
        )
    })?;

    // 검증 & 정규화 (PRD §6.3)
    let title = contract.title.trim().to_string();
    let summary = contract.summary.trim().to_string();
    let detail = contract.detail_markdown.trim().to_string();
    if title.is_empty() || summary.is_empty() || detail.is_empty() {
        return Err(AiError::new(
            "AI_BAD_CONTRACT",
            "필수 필드(title/summary/detail)가 비어 있습니다.",
        ));
    }
    let tags: Vec<String> = contract
        .tags
        .into_iter()
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty() && t.chars().count() <= 30)
        .take(8)
        .collect();
    let confidence = contract.confidence_suggestion.clamp(1, 3);

    Ok(GenerateResult {
        note: NoteOut {
            title: title.chars().take(120).collect(),
            summary: summary.chars().take(400).collect(),
            detail_markdown: detail,
            tags,
            confidence_suggestion: confidence,
            source_excerpt: contract.source_excerpt.filter(|s| !s.trim().is_empty()),
        },
        meta,
    })
}

/// claude CLI 사전 헬스체크: 경로/설치 확인 (버전 문자열 반환)
#[tauri::command]
pub async fn ai_health(cli_path: Option<String>) -> Result<String, AiError> {
    let program = cli_path.unwrap_or_else(|| "claude".to_string());
    let res = timeout(
        Duration::from_secs(HEALTH_TIMEOUT_SECS),
        Command::new(&program)
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        AiError::new(
            "AI_TIMEOUT",
            format!("{HEALTH_TIMEOUT_SECS}초 안에 응답이 없습니다."),
        )
    })?;
    let output = res.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AiError::new(
                "AI_NOT_FOUND",
                format!("claude 를 찾을 수 없습니다: {program}"),
            )
        } else {
            AiError::new("SPAWN_ERROR", e.to_string())
        }
    })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(AiError::new(
            "AI_ERROR",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}
