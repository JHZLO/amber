// DB 스키마 연동 — MySQL 접속 · information_schema introspection · 키체인.
//
// 다이어그램 탭이 DB 에 하는 일은 이 파일이 전부다: 아래 SQL 상수 여섯 문장(전부 information_schema
// SELECT)과 `SELECT VERSION()`. 사용자 입력이 SQL 로 가는 경로는 없다(바인딩 파라미터는 스키마 이름 하나).
// 세션은 시작 시 READ ONLY 로 잠근다 — 계정이 쓰기 권한을 가졌더라도 한 겹 더.
//
// 비밀번호는 macOS 키체인(`security` CLI, 아래 "키체인" 절)에서 꺼내 접속 옵션에 바로 꽂는다. URL 문자열을
// 조립하지 않으므로(인코딩 버그·로그 노출 원천 차단) 어디에도 문자열로 남지 않고, 에러 `detail` 에도 실리지 않는다.
// 계정 명령이 있는 연결은 키체인 대신 스키마마다 명령을 실행해 계정을 받는다(아래 "계정 명령" 절).
//
// 스냅샷의 형태는 프론트 `src/lib/schemaSnapshot.ts` 와 1:1 — 지문(fingerprint)은 프론트가 붙인다.

use serde::{Deserialize, Serialize};
use sqlx::mysql::{
    MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlPoolOptions, MySqlRow, MySqlSslMode,
};
use sqlx::{Connection, Executor, Row};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

use crate::ai::AiError;

/// 키체인 service. account 는 `db/<ulid>` — 연결마다 항목 하나.
const KEYCHAIN_SERVICE: &str = "dev.jhzlo.amber";
const CONNECT_TIMEOUT_SECS: u64 = 5;
const QUERY_TIMEOUT_SECS: u64 = 20;
/// 연결당 풀 크기 — 프론트가 스키마 셋을 동시에 introspect 한다(DiagramsView.autoSyncMissing). 그보다 하나 여유.
const POOL_MAX: u32 = 4;
/// 스키마 목록에서 늘 숨기는 MySQL 시스템 스키마
const SYSTEM_SCHEMAS: &[&str] = &["mysql", "sys", "performance_schema", "information_schema"];

// ---- information_schema 쿼리 (Amber 가 DB 에 보내는 SQL 의 전부) ----
//
// 숫자 컬럼은 전부 SIGNED 로 CAST 한다 — MySQL 8 의 information_schema 는 데이터 딕셔너리 뷰라
// TABLE_ROWS·ORDINAL_POSITION 이 BIGINT/INT UNSIGNED 로 오고, sqlx 는 부호가 다른 정수 디코딩을 거부한다.

const SQL_VERSION: &str = "SELECT VERSION()";

const SQL_SCHEMAS: &str = "SELECT s.SCHEMA_NAME, CAST(COUNT(t.TABLE_NAME) AS SIGNED) AS TABLES \
    FROM information_schema.SCHEMATA s \
    LEFT JOIN information_schema.TABLES t \
      ON t.TABLE_SCHEMA = s.SCHEMA_NAME AND t.TABLE_TYPE = 'BASE TABLE' \
    GROUP BY s.SCHEMA_NAME ORDER BY s.SCHEMA_NAME";

const SQL_TABLES: &str =
    "SELECT TABLE_NAME, TABLE_COMMENT, CAST(TABLE_ROWS AS SIGNED) AS TABLE_ROWS \
    FROM information_schema.TABLES \
    WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME";

const SQL_COLUMNS: &str =
    "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, \
        COLUMN_DEFAULT, EXTRA, COLUMN_COMMENT \
    FROM information_schema.COLUMNS \
    WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION";

const SQL_INDEXES: &str =
    "SELECT TABLE_NAME, INDEX_NAME, CAST(NON_UNIQUE AS SIGNED) AS NON_UNIQUE, COLUMN_NAME \
    FROM information_schema.STATISTICS \
    WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX";

const SQL_FOREIGN_KEYS: &str = "SELECT k.TABLE_NAME, k.CONSTRAINT_NAME, k.COLUMN_NAME, \
        k.REFERENCED_TABLE_SCHEMA, k.REFERENCED_TABLE_NAME, k.REFERENCED_COLUMN_NAME \
    FROM information_schema.KEY_COLUMN_USAGE k \
    JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
      ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
     AND r.TABLE_NAME = k.TABLE_NAME \
    WHERE k.TABLE_SCHEMA = ? AND k.REFERENCED_TABLE_NAME IS NOT NULL \
    ORDER BY k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION";

// CHECK_CONSTRAINTS 는 MySQL 8.0.16+ 에만 있다 — 없는 서버(MariaDB·5.7)에선 실패를 '제약 없음'으로 삼킨다.
const SQL_CHECKS: &str = "SELECT tc.TABLE_NAME, tc.CONSTRAINT_NAME, cc.CHECK_CLAUSE \
    FROM information_schema.TABLE_CONSTRAINTS tc \
    JOIN information_schema.CHECK_CONSTRAINTS cc \
      ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
    WHERE tc.TABLE_SCHEMA = ? AND tc.CONSTRAINT_TYPE = 'CHECK' \
    ORDER BY tc.TABLE_NAME, tc.CONSTRAINT_NAME";

// ---- 입력/출력 타입 (프론트 lib/dbconn.ts · lib/schemaSnapshot.ts 와 1:1) ----

/// 연결 프로필 — db_connections 행에서 접속에 필요한 것만. 비밀번호는 여기 없다(키체인).
#[derive(Debug, Clone, Deserialize)]
pub struct DbProfile {
    pub ulid: String,
    /// "mysql" 만 지원. 다른 값은 DB_UNSUPPORTED.
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// disabled | preferred | required
    #[serde(default = "default_tls")]
    pub tls: String,
    /// 비어 있으면 username + 키체인. 있으면 스키마마다 이 명령으로 계정을 받는다(username 은 쓰지 않는다)
    #[serde(default)]
    pub credential_command: String,
}

fn default_tls() -> String {
    "preferred".to_string()
}

#[derive(Debug, Serialize)]
pub struct SchemaInfo {
    pub name: String,
    pub tables: i64,
    /// 계정 명령 연결에서 이 스키마만 붙지 못했을 때. 비밀번호 연결은 늘 없다
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AiError>,
}

#[derive(Debug, Serialize)]
pub struct DbTestResult {
    /// 'MySQL 8.0.36' 처럼 표시용
    pub server: String,
    pub latency_ms: u64,
    /// 시스템 스키마를 뺀 목록 (이름순)
    pub schemas: Vec<SchemaInfo>,
}

#[derive(Debug, Serialize, Default)]
pub struct SnapshotColumn {
    pub name: String,
    pub data_type: String,
    pub column_type: String,
    pub nullable: bool,
    pub key: String,
    pub default_value: Option<String>,
    pub extra: String,
    pub comment: String,
}

#[derive(Debug, Serialize)]
pub struct SnapshotIndex {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotCheck {
    pub name: String,
    pub clause: String,
}

#[derive(Debug, Serialize)]
pub struct SnapshotTable {
    pub name: String,
    pub comment: String,
    pub rows_estimate: Option<i64>,
    pub columns: Vec<SnapshotColumn>,
    pub indexes: Vec<SnapshotIndex>,
    pub foreign_keys: Vec<SnapshotForeignKey>,
    pub checks: Vec<SnapshotCheck>,
}

/// 프론트 `RawSnapshot` — fingerprint 는 프론트가 계산해 붙인다
#[derive(Debug, Serialize)]
pub struct RawSnapshot {
    pub connection: String,
    pub schema: String,
    pub server: String,
    pub synced_at: i64,
    pub tables: Vec<SnapshotTable>,
}

// ---- 키체인 ----
//
// 저장·읽기·삭제를 전부 macOS 의 `/usr/bin/security` 로 한다 — 앱 바이너리가 직접 Security 프레임워크를
// 부르지 않는 이유가 이 기능의 핵심이다. 키체인 항목은 "만든 앱"만 조용히 읽을 수 있고 다른 서명의 앱이
// 읽으면 로그인 키체인 암호를 묻는 창이 뜬다. 개발 빌드는 빌드마다 다른 앱이라 그 창이 매번 떴다(실측).
// `security` 가 만들고 `security` 가 읽으면 접근 주체가 늘 같은(Apple 서명) 바이너리라 창이 뜨지 않는다 —
// git·docker 의 osxkeychain 자격 증명 도우미가 같은 방식이다.
// 그래도 프로세스가 사는 동안은 연결마다 한 번만 두드린다(메모리 캐시 + 풀 재사용).

const SECURITY_BIN: &str = "/usr/bin/security";
/// `security` 가 접근 확인창을 띄우면 사용자가 로그인 키체인 암호를 치는 시간이 여기 든다.
/// 20초로 잘랐을 때 암호를 치는 동안 프로세스가 죽어 실패 처리되고 창만 남았다(실측) — 넉넉히 5분.
const KEYCHAIN_TIMEOUT_SECS: u64 = 300;
/// 키체인 앱에서 보이는 항목 이름
const KEYCHAIN_LABEL: &str = "Amber DB connection";
/// `security` 의 errSecItemNotFound 종료 코드
const SEC_ITEM_NOT_FOUND: i32 = 44;

static SECRETS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
/// 키체인 읽기는 한 번에 하나만 — 여러 스키마 동기화가 겹치면 확인창이 겹겹이 뜬다. 먼저 읽은 값이
/// 캐시에 들어가면 뒤에 기다리던 쪽은 키체인을 두드리지 않는다.
static READ_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn secrets() -> &'static Mutex<HashMap<String, String>> {
    SECRETS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_secret(ulid: &str, password: &str) {
    if let Ok(mut m) = secrets().lock() {
        m.insert(ulid.to_string(), password.to_string());
    }
}

fn forget_secret(ulid: &str) {
    if let Ok(mut m) = secrets().lock() {
        m.remove(ulid);
    }
}

fn recall_secret(ulid: &str) -> Option<String> {
    secrets().lock().ok().and_then(|m| m.get(ulid).cloned())
}

fn keychain_account(ulid: &str) -> String {
    format!("db/{ulid}")
}

fn keychain_denied(detail: impl Into<String>) -> AiError {
    AiError::detailed("KEYCHAIN_DENIED", "키체인에 접근하지 못했습니다.", detail)
}

/// `security <args>` 실행. 셸을 거치지 않으므로 비밀번호가 히스토리·확장에 노출되지 않는다.
async fn security(args: &[&str]) -> Result<std::process::Output, AiError> {
    timeout(
        Duration::from_secs(KEYCHAIN_TIMEOUT_SECS),
        tokio::process::Command::new(SECURITY_BIN)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| keychain_denied(format!("{KEYCHAIN_TIMEOUT_SECS}s timeout")))?
    .map_err(|e| keychain_denied(e.to_string()))
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

async fn keychain_read(ulid: &str) -> Result<String, AiError> {
    let account = keychain_account(ulid);
    let out = security(&[
        "find-generic-password",
        "-s",
        KEYCHAIN_SERVICE,
        "-a",
        &account,
        "-w",
    ])
    .await?;
    if out.status.code() == Some(SEC_ITEM_NOT_FOUND) {
        return Err(AiError::new(
            "KEYCHAIN_MISSING",
            "저장된 비밀번호가 없습니다. 연결 설정에서 비밀번호를 다시 입력해 주세요.",
        ));
    }
    if !out.status.success() {
        return Err(keychain_denied(stderr_of(&out)));
    }
    // -w 는 값 뒤에 개행 하나를 붙인다. 비밀번호 안의 공백은 살려야 하므로 개행만 뗀다
    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    Ok(raw.trim_end_matches(['\n', '\r']).to_string())
}

/// 항목을 지우고 새로 만든다 — 다른 바이너리(예전 Amber)가 만든 항목을 `-U` 로 고치면 그 항목의 접근 제어가
/// 남아 다음 읽기에 창이 뜬다. 지우기는 접근 제어를 타지 않는다.
async fn keychain_write(ulid: &str, password: &str) -> Result<(), AiError> {
    let account = keychain_account(ulid);
    let _ = security(&[
        "delete-generic-password",
        "-s",
        KEYCHAIN_SERVICE,
        "-a",
        &account,
    ])
    .await;
    let out = security(&[
        "add-generic-password",
        "-s",
        KEYCHAIN_SERVICE,
        "-a",
        &account,
        "-l",
        KEYCHAIN_LABEL,
        "-w",
        password,
        "-U",
    ])
    .await?;
    if !out.status.success() {
        return Err(keychain_denied(stderr_of(&out)));
    }
    Ok(())
}

/// 없는 항목 삭제는 멱등 성공
async fn keychain_delete(ulid: &str) -> Result<(), AiError> {
    let account = keychain_account(ulid);
    let out = security(&[
        "delete-generic-password",
        "-s",
        KEYCHAIN_SERVICE,
        "-a",
        &account,
    ])
    .await?;
    if out.status.success() || out.status.code() == Some(SEC_ITEM_NOT_FOUND) {
        return Ok(());
    }
    Err(keychain_denied(stderr_of(&out)))
}

/// 이 연결의 비밀번호 — 메모리에 있으면 그것, 없으면 키체인에서 한 번 읽어 기억한다.
async fn password_for(ulid: &str) -> Result<String, AiError> {
    if let Some(p) = recall_secret(ulid) {
        return Ok(p);
    }
    let _guard = READ_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    // 기다리는 동안 다른 호출이 읽어 두었을 수 있다
    if let Some(p) = recall_secret(ulid) {
        return Ok(p);
    }
    let p = keychain_read(ulid).await?;
    remember_secret(ulid, &p);
    Ok(p)
}

/// 비밀번호를 키체인에 저장한다. IPC 를 지나는 유일한 순간 — 이후 프론트는 비밀번호를 모른다.
/// 프로필이 바뀌었을 수 있으니 이 연결의 풀도 버린다(다음 접속이 새 비밀번호로 열리게).
#[tauri::command]
pub async fn db_secret_set(ulid: String, password: String) -> Result<(), AiError> {
    if password.is_empty() {
        return Err(AiError::new(
            "DB_EMPTY_PASSWORD",
            "비밀번호를 입력해 주세요.",
        ));
    }
    drop_pool(&ulid);
    forget_secret(&ulid);
    keychain_write(&ulid, &password).await?;
    // 방금 저장한 값은 이미 안다 — 곧 이어지는 첫 동기화가 키체인을 다시 두드리지 않게
    remember_secret(&ulid, &password);
    Ok(())
}

/// 키체인 항목 삭제 (연결 삭제 시). 항목이 없으면 멱등 성공.
#[tauri::command]
pub async fn db_secret_delete(ulid: String) -> Result<(), AiError> {
    drop_pool(&ulid);
    forget_secret(&ulid);
    keychain_delete(&ulid).await
}

// "비밀번호가 키체인에 있나"를 따로 묻는 커맨드는 두지 않는다 — 없는 상태는 동기화가 KEYCHAIN_MISSING 으로
// 실패하며 드러나고, 프론트가 그 코드를 보고 "비밀번호 필요"로 표시한다(백업을 다른 기기에 복원한 경우).

// ---- 계정 명령 ----
//
// 서비스마다 계정이 따로 있고 그 계정은 자기 스키마만 보는 DB 를 위한 길이다. 비밀번호가 비밀 저장소에서
// 바뀌기도 해서 키체인에 사본을 둘 수 없다. 연결에 명령을 하나 두고 스키마마다 실행해 stdout 의
// {"username","password"} 를 받는다(AWS credential_process 와 같은 꼴). 셸을 거치지 않는다: 공백으로 나누고
// `{schema}` 를 스키마 이름으로, 앞의 `~/` 를 홈으로 바꾼다. PATH 는 앱의 셸 PATH(shellenv).
// 받은 값은 접속 옵션에만 넣는다. 키체인, 디스크, 에러 detail 어디에도 남기지 않는다.

/// 비밀 저장소 호출과 터널 확인이 들어가는 명령이라 키체인보다 넉넉히
const CREDENTIAL_TIMEOUT_SECS: u64 = 60;
/// 연결 확인에서 계정 명령을 동시에 몇 개까지 — 스키마가 수십 개라 하나씩이면 1분을 넘긴다
const CREDENTIAL_PARALLEL: usize = 6;

#[derive(Deserialize)]
struct CommandCredential {
    username: String,
    password: String,
}

fn uses_command(profile: &DbProfile) -> bool {
    !profile.credential_command.trim().is_empty()
}

fn credential_failed(detail: impl Into<String>) -> AiError {
    AiError::detailed("DB_CREDENTIAL", "계정 명령이 실패했습니다.", detail)
}

/// 명령 문자열 → 실행할 argv
fn command_argv(command: &str, schema: &str, home: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| {
            let part = part.replace("{schema}", schema);
            match part.strip_prefix("~/") {
                Some(rest) if !home.is_empty() => format!("{home}/{rest}"),
                _ => part,
            }
        })
        .collect()
}

async fn command_credential(command: &str, schema: &str) -> Result<(String, String), AiError> {
    let home = std::env::var("HOME").unwrap_or_default();
    let argv = command_argv(command, schema, &home);
    let Some((program, args)) = argv.split_first() else {
        return Err(credential_failed(schema));
    };
    let mut cmd = crate::shellenv::command(program).await;
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let out = timeout(Duration::from_secs(CREDENTIAL_TIMEOUT_SECS), cmd.output())
        .await
        .map_err(|_| credential_failed(format!("{schema}: {CREDENTIAL_TIMEOUT_SECS}s timeout")))?
        .map_err(|e| credential_failed(format!("{schema}: {e}")))?;
    if !out.status.success() {
        // stderr 의 마지막 줄이 대개 원인이다. stdout 은 비밀번호를 담을 수 있으니 싣지 않는다
        let stderr = stderr_of(&out);
        let reason = stderr
            .lines()
            .last()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| out.status.to_string());
        return Err(credential_failed(format!("{schema}: {reason}")));
    }
    let c: CommandCredential = serde_json::from_slice(&out.stdout).map_err(|_| {
        credential_failed(format!(
            "{schema}: stdout is not {{\"username\", \"password\"}} JSON"
        ))
    })?;
    if c.username.trim().is_empty() || c.password.is_empty() {
        return Err(credential_failed(format!(
            "{schema}: empty username or password"
        )));
    }
    Ok((c.username.trim().to_string(), c.password))
}

/// 이 스키마에 붙을 계정. 명령이 있으면 스키마마다 명령으로, 없으면 프로필 사용자 + 키체인
async fn credential_for(profile: &DbProfile, schema: &str) -> Result<(String, String), AiError> {
    if uses_command(profile) {
        command_credential(&profile.credential_command, schema).await
    } else {
        Ok((
            profile.username.trim().to_string(),
            password_for(&profile.ulid).await?,
        ))
    }
}

// ---- 접속 ----

fn ssl_mode(tls: &str) -> MySqlSslMode {
    match tls {
        "disabled" => MySqlSslMode::Disabled,
        "required" => MySqlSslMode::Required,
        _ => MySqlSslMode::Preferred,
    }
}

fn connect_options(
    profile: &DbProfile,
    username: &str,
    password: &str,
) -> Result<MySqlConnectOptions, AiError> {
    if profile.kind != "mysql" {
        return Err(AiError::detailed(
            "DB_UNSUPPORTED",
            "지원하지 않는 데이터베이스 종류입니다.",
            profile.kind.clone(),
        ));
    }
    let host = profile.host.trim();
    if host.is_empty() || username.trim().is_empty() {
        return Err(AiError::new(
            "DB_EMPTY_FIELD",
            "호스트와 사용자를 입력해 주세요.",
        ));
    }
    Ok(MySqlConnectOptions::new()
        .host(host)
        .port(profile.port)
        .username(username.trim())
        .password(password)
        .ssl_mode(ssl_mode(&profile.tls))
        // information_schema 만 읽으므로 기본 database 를 고르지 않는다 — 특정 스키마 권한이 없어도 접속된다
        .statement_cache_capacity(0))
}

/// sqlx 에러 → 코드화 에러. 문구는 프론트(`lib/errors.ts`)가 만들고 여기 message 는 폴백이다.
/// MySQL 에러 번호가 원인을 가장 정확히 말한다: 1045 인증, 1044/1049 스키마 권한·부재.
fn map_sqlx(e: sqlx::Error) -> AiError {
    match &e {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.to_string()).unwrap_or_default();
            let msg = db.message().to_string();
            match code.as_str() {
                "1045" | "1698" | "1251" => {
                    AiError::detailed("DB_AUTH", "DB 인증에 실패했습니다.", msg)
                }
                "1044" | "1049" | "1142" => AiError::detailed(
                    "DB_SCHEMA_DENIED",
                    "이 스키마를 읽을 권한이 없거나 존재하지 않습니다.",
                    msg,
                ),
                "3159" | "1105" if msg.to_lowercase().contains("ssl") => {
                    AiError::detailed("DB_TLS", "TLS 협상에 실패했습니다.", msg)
                }
                _ => AiError::detailed("DB_QUERY", "DB 조회에 실패했습니다.", msg),
            }
        }
        sqlx::Error::Io(io) => match io.kind() {
            std::io::ErrorKind::TimedOut => AiError::detailed(
                "DB_TIMEOUT",
                "DB 응답이 없습니다.",
                CONNECT_TIMEOUT_SECS.to_string(),
            ),
            _ => AiError::detailed(
                "DB_REFUSED",
                "DB 에 연결할 수 없습니다. 포트 포워딩·터널이 살아 있는지 확인하세요.",
                io.to_string(),
            ),
        },
        sqlx::Error::Tls(t) => {
            AiError::detailed("DB_TLS", "TLS 협상에 실패했습니다.", t.to_string())
        }
        sqlx::Error::PoolTimedOut => AiError::detailed(
            "DB_TIMEOUT",
            "DB 응답이 없습니다.",
            CONNECT_TIMEOUT_SECS.to_string(),
        ),
        sqlx::Error::Protocol(p) => {
            AiError::detailed("DB_QUERY", "DB 프로토콜 오류입니다.", p.clone())
        }
        other => AiError::detailed("DB_QUERY", "DB 조회에 실패했습니다.", other.to_string()),
    }
}

fn timed_out(secs: u64) -> AiError {
    AiError::detailed("DB_TIMEOUT", "DB 응답이 없습니다.", secs.to_string())
}

/// 풀 캐시 — 같은 프로필(호스트, 포트, 사용자, TLS, 계정 명령)이면 재사용, 달라졌으면 새로 만든다.
/// 비밀번호 변경은 db_secret_set 이 drop_pool 로 비운다.
static POOLS: OnceLock<Mutex<HashMap<String, (String, MySqlPool)>>> = OnceLock::new();

fn pools() -> &'static Mutex<HashMap<String, (String, MySqlPool)>> {
    POOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 풀 캐시의 키. 비밀번호 연결은 연결 하나에 풀 하나, 계정 명령 연결은 스키마마다 계정이 달라 스키마마다 하나
fn pool_key(profile: &DbProfile, schema: &str) -> String {
    if uses_command(profile) {
        format!("{}/{schema}", profile.ulid)
    } else {
        profile.ulid.clone()
    }
}

/// 이 연결의 풀 전부(스키마별 풀 포함)
fn drop_pool(ulid: &str) {
    let prefix = format!("{ulid}/");
    if let Ok(mut m) = pools().lock() {
        m.retain(|k, _| k != ulid && !k.starts_with(&prefix));
    }
}

fn drop_pool_key(key: &str) {
    if let Ok(mut m) = pools().lock() {
        m.remove(key);
    }
}

fn profile_sig(p: &DbProfile) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        p.kind,
        p.host.trim(),
        p.port,
        p.username.trim(),
        p.tls,
        p.credential_command.trim()
    )
}

/// 같은 프로필로 이미 만든 풀
fn cached_pool(profile: &DbProfile, key: &str) -> Option<MySqlPool> {
    let sig = profile_sig(profile);
    let m = pools().lock().ok()?;
    let (s, pool) = m.get(key)?;
    (*s == sig).then(|| pool.clone())
}

fn pool_for(
    profile: &DbProfile,
    key: &str,
    username: &str,
    password: &str,
) -> Result<MySqlPool, AiError> {
    if let Some(pool) = cached_pool(profile, key) {
        return Ok(pool);
    }
    let opts = connect_options(profile, username, password)?;
    let pool = MySqlPoolOptions::new()
        .max_connections(POOL_MAX)
        .acquire_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        // 계정에 쓰기 권한이 있어도 이 세션으론 못 쓰게 — 읽기 전용 약속을 코드로 못 박는다
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET SESSION TRANSACTION READ ONLY").await?;
                Ok(())
            })
        })
        .connect_lazy_with(opts);
    if let Ok(mut m) = pools().lock() {
        m.insert(key.to_string(), (profile_sig(profile), pool.clone()));
    }
    Ok(pool)
}

// ---- 행 디코딩 ----
//
// information_schema 의 문자열 컬럼은 서버·버전에 따라 TEXT 계열이거나 바이너리 콜레이션으로 온다.
// 한 가지 타입으로 try_get 하면 특정 서버에서만 터지므로 문자열 → 바이트 순으로 시도한다.

fn str_at(row: &MySqlRow, i: usize) -> String {
    opt_str_at(row, i).unwrap_or_default()
}

fn opt_str_at(row: &MySqlRow, i: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v;
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v.map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    None
}

fn i64_at(row: &MySqlRow, i: usize) -> Option<i64> {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v;
    }
    if let Ok(v) = row.try_get::<Option<u64>, _>(i) {
        return v.map(|n| n as i64);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(i) {
        return v.map(i64::from);
    }
    None
}

async fn fetch_all(pool: &MySqlPool, sql: &str, schema: &str) -> Result<Vec<MySqlRow>, AiError> {
    timeout(
        Duration::from_secs(QUERY_TIMEOUT_SECS),
        sqlx::query(sql).bind(schema).fetch_all(pool),
    )
    .await
    .map_err(|_| timed_out(QUERY_TIMEOUT_SECS))?
    .map_err(map_sqlx)
}

fn server_label(version: &str) -> String {
    let v = version.trim();
    if v.to_lowercase().contains("mariadb") {
        format!("MariaDB {}", v.split('-').next().unwrap_or(v))
    } else {
        format!("MySQL {v}")
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- 커맨드: 연결 테스트 ----

/// 저장 전 연결 확인. 비밀번호를 안 주면 키체인에서 읽는다(편집 모달·설정의 재검사).
/// 계정 명령 연결은 계정마다 자기 스키마만 보이므로 `schemas` 로 받은 스키마마다 따로 붙어 본다.
#[tauri::command]
pub async fn db_test(
    profile: DbProfile,
    password: Option<String>,
    schemas: Option<Vec<String>>,
) -> Result<DbTestResult, AiError> {
    if uses_command(&profile) {
        return test_by_command(profile, schemas.unwrap_or_default()).await;
    }
    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ => password_for(&profile.ulid).await?,
    };
    let started = Instant::now();
    let (server, schemas) = probe(&profile, &profile.username, &password).await?;
    Ok(DbTestResult {
        server,
        latency_ms: started.elapsed().as_millis() as u64,
        schemas,
    })
}

/// 계정 명령 연결의 확인. 스키마마다 계정을 받아 붙어 보고, 붙지 못한 스키마는 이유를 달아 돌려준다
/// (수십 개 중 하나 때문에 확인 전체가 막히지 않게). 하나도 못 붙으면 첫 실패로 실패한다.
async fn test_by_command(
    profile: DbProfile,
    schemas: Vec<String>,
) -> Result<DbTestResult, AiError> {
    let mut names: Vec<String> = schemas
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Err(AiError::new(
            "DB_EMPTY_FIELD",
            "스키마 이름을 입력해 주세요.",
        ));
    }
    let started = Instant::now();
    let gate = Arc::new(tokio::sync::Semaphore::new(CREDENTIAL_PARALLEL));
    let tasks: Vec<_> = names
        .into_iter()
        .map(|name| {
            let (profile, gate, schema) = (profile.clone(), gate.clone(), name.clone());
            let task = tokio::spawn(async move {
                let _permit = gate.acquire_owned().await;
                probe_schema(&profile, &schema).await
            });
            (name, task)
        })
        .collect();

    let mut server = None;
    let mut schemas = Vec::with_capacity(tasks.len());
    for (name, task) in tasks {
        let probed = task
            .await
            .unwrap_or_else(|e| Err(credential_failed(format!("{name}: {e}"))));
        match probed {
            Ok((label, tables)) => {
                server.get_or_insert(label);
                schemas.push(SchemaInfo {
                    name,
                    tables,
                    error: None,
                });
            }
            Err(e) => schemas.push(SchemaInfo {
                name,
                tables: 0,
                error: Some(e),
            }),
        }
    }
    let Some(server) = server else {
        return Err(schemas
            .into_iter()
            .find_map(|s| s.error)
            .unwrap_or_else(|| credential_failed("")));
    };
    Ok(DbTestResult {
        server,
        latency_ms: started.elapsed().as_millis() as u64,
        schemas,
    })
}

/// 계정 명령으로 이 스키마의 계정을 받아 붙어 본다 → (서버, 테이블 수)
async fn probe_schema(profile: &DbProfile, schema: &str) -> Result<(String, i64), AiError> {
    let (username, password) = command_credential(&profile.credential_command, schema).await?;
    let (server, visible) = probe(profile, &username, &password).await?;
    visible
        .into_iter()
        .find(|s| s.name == schema)
        .map(|s| (server, s.tables))
        // 붙기는 했는데 그 스키마가 안 보인다: 다른 스키마의 계정이거나 이름이 틀렸다
        .ok_or_else(|| {
            AiError::detailed(
                "DB_SCHEMA_DENIED",
                "이 스키마를 읽을 권한이 없거나 존재하지 않습니다.",
                schema.to_string(),
            )
        })
}

/// 단발 연결로 서버 버전과 보이는 스키마 목록을 읽는다 → (서버 표시명, 시스템 스키마를 뺀 목록).
/// 풀을 거치지 않는다: 아직 저장되지 않은 프로필로 캐시를 오염시키지 않는다.
async fn probe(
    profile: &DbProfile,
    username: &str,
    password: &str,
) -> Result<(String, Vec<SchemaInfo>), AiError> {
    let opts = connect_options(profile, username, password)?;
    let mut conn = timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        MySqlConnection::connect_with(&opts),
    )
    .await
    .map_err(|_| timed_out(CONNECT_TIMEOUT_SECS))?
    .map_err(map_sqlx)?;

    let version: String = timeout(
        Duration::from_secs(QUERY_TIMEOUT_SECS),
        sqlx::query_scalar(SQL_VERSION).fetch_one(&mut conn),
    )
    .await
    .map_err(|_| timed_out(QUERY_TIMEOUT_SECS))?
    .map_err(map_sqlx)?;

    let rows = timeout(
        Duration::from_secs(QUERY_TIMEOUT_SECS),
        sqlx::query(SQL_SCHEMAS).fetch_all(&mut conn),
    )
    .await
    .map_err(|_| timed_out(QUERY_TIMEOUT_SECS))?
    .map_err(map_sqlx)?;
    let _ = conn.close().await;

    let schemas = rows
        .iter()
        .map(|r| SchemaInfo {
            name: str_at(r, 0),
            tables: i64_at(r, 1).unwrap_or(0),
            error: None,
        })
        .filter(|s| !SYSTEM_SCHEMAS.contains(&s.name.as_str()))
        .collect();

    Ok((server_label(&version), schemas))
}

// ---- 커맨드: introspection ----

/// 스키마 하나의 구조 스냅샷. 여러 스키마는 프론트가 순차 호출한다(진행 표시가 트리 행 단위라 자연스럽다).
/// 취소 커맨드는 두지 않는다 — 다섯 쿼리 각각 20초 상한이고 실제로는 터널 너머에서도 1초 안에 끝난다.
#[tauri::command]
pub async fn db_introspect(profile: DbProfile, schema: String) -> Result<RawSnapshot, AiError> {
    let schema = schema.trim().to_string();
    if schema.is_empty() {
        return Err(AiError::new(
            "DB_EMPTY_FIELD",
            "스키마 이름이 비어 있습니다.",
        ));
    }
    // 같은 프로필의 풀이 살아 있으면 계정이 필요 없다 — 키체인은 첫 스키마에서만, 계정 명령은 스키마마다 한 번
    let key = pool_key(&profile, &schema);
    let pool = match cached_pool(&profile, &key) {
        Some(p) => p,
        None => {
            let (username, password) = credential_for(&profile, &schema).await?;
            pool_for(&profile, &key, &username, &password)?
        }
    };

    let version: String = timeout(
        Duration::from_secs(QUERY_TIMEOUT_SECS),
        sqlx::query_scalar(SQL_VERSION).fetch_one(&pool),
    )
    .await
    .map_err(|_| timed_out(QUERY_TIMEOUT_SECS))?
    .map_err(map_sqlx)
    // 계정 명령이 준 비밀번호는 비밀 저장소에서 바뀐다. 거절되면 풀을 버려 다음 동기화가 새로 받게 한다
    .map_err(|e| {
        if e.code == "DB_AUTH" {
            drop_pool_key(&key);
        }
        e
    })?;

    // 테이블 골격 (이름순 — 프론트 생성기도 이 순서를 그대로 쓴다)
    let mut tables: Vec<SnapshotTable> = fetch_all(&pool, SQL_TABLES, &schema)
        .await?
        .iter()
        .map(|r| SnapshotTable {
            name: str_at(r, 0),
            comment: str_at(r, 1),
            rows_estimate: i64_at(r, 2),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            checks: Vec::new(),
        })
        .collect();
    if tables.is_empty() {
        // 권한이 없는 스키마는 에러가 아니라 빈 목록으로 온다 — 사용자에게는 같은 뜻이라 코드로 알린다
        return Err(AiError::detailed(
            "DB_SCHEMA_DENIED",
            "이 스키마에 읽을 수 있는 테이블이 없습니다.",
            schema,
        ));
    }
    let index_of: HashMap<String, usize> = tables
        .iter()
        .enumerate()
        .map(|(i, t)| (t.name.clone(), i))
        .collect();

    // 컬럼 (ORDINAL_POSITION 순)
    for r in fetch_all(&pool, SQL_COLUMNS, &schema).await? {
        let Some(&ti) = index_of.get(&str_at(&r, 0)) else {
            continue;
        };
        tables[ti].columns.push(SnapshotColumn {
            name: str_at(&r, 1),
            data_type: str_at(&r, 2).to_lowercase(),
            column_type: str_at(&r, 3),
            nullable: str_at(&r, 4).eq_ignore_ascii_case("YES"),
            key: str_at(&r, 5),
            default_value: opt_str_at(&r, 6),
            extra: str_at(&r, 7),
            comment: str_at(&r, 8),
        });
    }

    // 인덱스 — (테이블, 인덱스명) 으로 묶고 컬럼은 SEQ_IN_INDEX 순으로 붙인다. 함수형 인덱스(컬럼 NULL)는 건너뛴다
    for r in fetch_all(&pool, SQL_INDEXES, &schema).await? {
        let Some(&ti) = index_of.get(&str_at(&r, 0)) else {
            continue;
        };
        let Some(col) = opt_str_at(&r, 3) else {
            continue;
        };
        let name = str_at(&r, 1);
        let unique = i64_at(&r, 2).unwrap_or(1) == 0;
        let t = &mut tables[ti];
        match t.indexes.iter_mut().find(|ix| ix.name == name) {
            Some(ix) => ix.columns.push(col),
            None => t.indexes.push(SnapshotIndex {
                name,
                unique,
                columns: vec![col],
            }),
        }
    }

    // 물리 FK — 제약 이름으로 묶는다(복합 FK 는 컬럼이 여러 개)
    for r in fetch_all(&pool, SQL_FOREIGN_KEYS, &schema).await? {
        let Some(&ti) = index_of.get(&str_at(&r, 0)) else {
            continue;
        };
        let name = str_at(&r, 1);
        let col = str_at(&r, 2);
        let ref_col = str_at(&r, 5);
        let t = &mut tables[ti];
        match t.foreign_keys.iter_mut().find(|fk| fk.name == name) {
            Some(fk) => {
                fk.columns.push(col);
                fk.ref_columns.push(ref_col);
            }
            None => t.foreign_keys.push(SnapshotForeignKey {
                name,
                columns: vec![col],
                ref_schema: str_at(&r, 3),
                ref_table: str_at(&r, 4),
                ref_columns: vec![ref_col],
            }),
        }
    }

    // CHECK — 8.0.16 미만·MariaDB 는 테이블이 없어 실패한다. enum 후보의 보조 근거일 뿐이라 없는 것으로 친다
    if let Ok(rows) = fetch_all(&pool, SQL_CHECKS, &schema).await {
        for r in rows {
            let Some(&ti) = index_of.get(&str_at(&r, 0)) else {
                continue;
            };
            tables[ti].checks.push(SnapshotCheck {
                name: str_at(&r, 1),
                clause: str_at(&r, 2),
            });
        }
    }

    Ok(RawSnapshot {
        connection: profile.ulid,
        schema,
        server: server_label(&version),
        synced_at: now_ms(),
        tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_label_distinguishes_mariadb() {
        assert_eq!(server_label("8.0.36"), "MySQL 8.0.36");
        assert_eq!(server_label("10.11.6-MariaDB-log"), "MariaDB 10.11.6");
    }

    #[test]
    fn profile_sig_ignores_whitespace_but_not_identity() {
        let a = DbProfile {
            ulid: "x".into(),
            kind: "mysql".into(),
            host: " 127.0.0.1 ".into(),
            port: 3307,
            username: "ro".into(),
            tls: "preferred".into(),
            credential_command: String::new(),
        };
        let mut b = DbProfile {
            host: "127.0.0.1".into(),
            ..a.clone()
        };
        assert_eq!(profile_sig(&a), profile_sig(&b));
        b.username = "rw".into();
        assert_ne!(profile_sig(&a), profile_sig(&b));
        let c = DbProfile {
            credential_command: "cred {schema}".into(),
            ..a.clone()
        };
        assert_ne!(profile_sig(&a), profile_sig(&c));
    }

    #[test]
    fn connect_options_reject_unknown_kind_and_blank_host() {
        let p = DbProfile {
            ulid: "x".into(),
            kind: "postgres".into(),
            host: "h".into(),
            port: 1,
            username: "u".into(),
            tls: "preferred".into(),
            credential_command: String::new(),
        };
        assert_eq!(
            connect_options(&p, "u", "pw").unwrap_err().code,
            "DB_UNSUPPORTED"
        );
        let q = DbProfile {
            kind: "mysql".into(),
            host: "  ".into(),
            ..p
        };
        assert_eq!(
            connect_options(&q, "u", "pw").unwrap_err().code,
            "DB_EMPTY_FIELD"
        );
    }

    #[test]
    fn command_argv_fills_schema_and_home_without_a_shell() {
        assert_eq!(
            command_argv("~/bin/cred  --db {schema} ~", "orders", "/Users/me"),
            vec!["/Users/me/bin/cred", "--db", "orders", "~"]
        );
        // 홈을 모르면 그대로 둔다(실행 시 NotFound 로 드러난다)
        assert_eq!(command_argv("~/cred", "x", ""), vec!["~/cred"]);
    }

    #[test]
    fn command_connections_pool_per_schema() {
        let p = DbProfile {
            ulid: "01J".into(),
            kind: "mysql".into(),
            host: "127.0.0.1".into(),
            port: 3307,
            username: "ro".into(),
            tls: "preferred".into(),
            credential_command: String::new(),
        };
        assert_eq!(pool_key(&p, "orders"), "01J");
        let q = DbProfile {
            credential_command: "cred {schema}".into(),
            ..p
        };
        assert_eq!(pool_key(&q, "orders"), "01J/orders");
    }

    #[test]
    fn keychain_account_is_namespaced_per_connection() {
        assert_eq!(keychain_account("01J"), "db/01J");
    }
}
