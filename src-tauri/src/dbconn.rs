// DB 스키마 연동 — MySQL 접속 · information_schema introspection · 키체인.
//
// 다이어그램 탭이 DB 에 하는 일은 이 파일이 전부다: 아래 SQL 상수 여섯 문장(전부 information_schema
// SELECT)과 `SELECT VERSION()`. 사용자 입력이 SQL 로 가는 경로는 없다(바인딩 파라미터는 스키마 이름 하나).
// 세션은 시작 시 READ ONLY 로 잠근다 — 계정이 쓰기 권한을 가졌더라도 한 겹 더.
//
// 비밀번호는 macOS 키체인에서 꺼내 접속 옵션에 바로 꽂는다. URL 문자열을 조립하지 않으므로(인코딩 버그·
// 로그 노출 원천 차단) 어디에도 문자열로 남지 않고, 에러 `detail` 에도 실리지 않는다.
//
// 스냅샷의 형태는 프론트 `src/lib/schemaSnapshot.ts` 와 1:1 — 지문(fingerprint)은 프론트가 붙인다.

use serde::{Deserialize, Serialize};
use sqlx::mysql::{
    MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlPoolOptions, MySqlRow, MySqlSslMode,
};
use sqlx::{Connection, Executor, Row};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

use crate::ai::AiError;

/// 키체인 service. account 는 `db/<ulid>` — 연결마다 항목 하나.
const KEYCHAIN_SERVICE: &str = "dev.jhzlo.amber";
const CONNECT_TIMEOUT_SECS: u64 = 5;
const QUERY_TIMEOUT_SECS: u64 = 20;
/// 연결당 풀 크기 — 여러 스키마를 잇달아 동기화할 때 재접속을 피하는 용도지 동시성이 목적이 아니다.
const POOL_MAX: u32 = 2;
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
}

fn default_tls() -> String {
    "preferred".to_string()
}

#[derive(Debug, Serialize)]
pub struct SchemaInfo {
    pub name: String,
    pub tables: i64,
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

fn keychain_account(ulid: &str) -> String {
    format!("db/{ulid}")
}

fn keychain_entry(ulid: &str) -> Result<keyring::Entry, AiError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &keychain_account(ulid)).map_err(|e| {
        AiError::detailed("KEYCHAIN_DENIED", "키체인을 열 수 없습니다.", e.to_string())
    })
}

fn keychain_read(ulid: &str) -> Result<String, AiError> {
    match keychain_entry(ulid)?.get_password() {
        Ok(p) => Ok(p),
        Err(keyring::Error::NoEntry) => Err(AiError::new(
            "KEYCHAIN_MISSING",
            "저장된 비밀번호가 없습니다. 연결 설정에서 비밀번호를 다시 입력해 주세요.",
        )),
        Err(e) => Err(AiError::detailed(
            "KEYCHAIN_DENIED",
            "키체인에서 비밀번호를 읽지 못했습니다.",
            e.to_string(),
        )),
    }
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
    tokio::task::spawn_blocking(move || {
        keychain_entry(&ulid)?.set_password(&password).map_err(|e| {
            AiError::detailed(
                "KEYCHAIN_DENIED",
                "키체인에 비밀번호를 저장하지 못했습니다.",
                e.to_string(),
            )
        })
    })
    .await
    .map_err(|e| AiError::detailed("KEYCHAIN_DENIED", e.to_string(), e.to_string()))?
}

/// 키체인 항목 삭제 (연결 삭제 시). 항목이 없으면 멱등 성공.
#[tauri::command]
pub async fn db_secret_delete(ulid: String) -> Result<(), AiError> {
    drop_pool(&ulid);
    tokio::task::spawn_blocking(move || match keychain_entry(&ulid)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AiError::detailed(
            "KEYCHAIN_DENIED",
            "키체인 항목을 지우지 못했습니다.",
            e.to_string(),
        )),
    })
    .await
    .map_err(|e| AiError::detailed("KEYCHAIN_DENIED", e.to_string(), e.to_string()))?
}

/// 비밀번호가 키체인에 있는가 — 백업을 다른 기기에 복원하면 프로필은 있는데 비밀번호가 없다.
/// 그 상태를 "연결 실패"가 아니라 "비밀번호 필요"로 이름 붙여 보여주기 위한 조회.
#[tauri::command]
pub async fn db_secret_exists(ulid: String) -> Result<bool, AiError> {
    tokio::task::spawn_blocking(move || match keychain_entry(&ulid)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(AiError::detailed(
            "KEYCHAIN_DENIED",
            "키체인을 읽지 못했습니다.",
            e.to_string(),
        )),
    })
    .await
    .map_err(|e| AiError::detailed("KEYCHAIN_DENIED", e.to_string(), e.to_string()))?
}

// ---- 접속 ----

fn ssl_mode(tls: &str) -> MySqlSslMode {
    match tls {
        "disabled" => MySqlSslMode::Disabled,
        "required" => MySqlSslMode::Required,
        _ => MySqlSslMode::Preferred,
    }
}

fn connect_options(profile: &DbProfile, password: &str) -> Result<MySqlConnectOptions, AiError> {
    if profile.kind != "mysql" {
        return Err(AiError::detailed(
            "DB_UNSUPPORTED",
            "지원하지 않는 데이터베이스 종류입니다.",
            profile.kind.clone(),
        ));
    }
    let host = profile.host.trim();
    if host.is_empty() || profile.username.trim().is_empty() {
        return Err(AiError::new(
            "DB_EMPTY_FIELD",
            "호스트와 사용자를 입력해 주세요.",
        ));
    }
    Ok(MySqlConnectOptions::new()
        .host(host)
        .port(profile.port)
        .username(profile.username.trim())
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

/// 연결별 풀 캐시 — 같은 프로필(호스트·포트·사용자·TLS)이면 재사용, 달라졌으면 새로 만든다.
/// 비밀번호 변경은 db_secret_set 이 drop_pool 로 비운다.
static POOLS: OnceLock<Mutex<HashMap<String, (String, MySqlPool)>>> = OnceLock::new();

fn pools() -> &'static Mutex<HashMap<String, (String, MySqlPool)>> {
    POOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn drop_pool(ulid: &str) {
    if let Ok(mut m) = pools().lock() {
        m.remove(ulid);
    }
}

fn profile_sig(p: &DbProfile) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        p.kind,
        p.host.trim(),
        p.port,
        p.username.trim(),
        p.tls
    )
}

fn pool_for(profile: &DbProfile, password: &str) -> Result<MySqlPool, AiError> {
    let sig = profile_sig(profile);
    if let Ok(m) = pools().lock() {
        if let Some((s, pool)) = m.get(&profile.ulid) {
            if *s == sig {
                return Ok(pool.clone());
            }
        }
    }
    let opts = connect_options(profile, password)?;
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
        m.insert(profile.ulid.clone(), (sig, pool.clone()));
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
/// 풀을 거치지 않는 단발 연결 — 아직 저장되지 않은 프로필로 캐시를 오염시키지 않는다.
#[tauri::command]
pub async fn db_test(
    profile: DbProfile,
    password: Option<String>,
) -> Result<DbTestResult, AiError> {
    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ => {
            let ulid = profile.ulid.clone();
            tokio::task::spawn_blocking(move || keychain_read(&ulid))
                .await
                .map_err(|e| AiError::detailed("KEYCHAIN_DENIED", e.to_string(), e.to_string()))??
        }
    };
    let opts = connect_options(&profile, &password)?;
    let started = Instant::now();
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
    let latency_ms = started.elapsed().as_millis() as u64;
    let _ = conn.close().await;

    let schemas = rows
        .iter()
        .map(|r| SchemaInfo {
            name: str_at(r, 0),
            tables: i64_at(r, 1).unwrap_or(0),
        })
        .filter(|s| !SYSTEM_SCHEMAS.contains(&s.name.as_str()))
        .collect();

    Ok(DbTestResult {
        server: server_label(&version),
        latency_ms,
        schemas,
    })
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
    let ulid = profile.ulid.clone();
    let password = tokio::task::spawn_blocking(move || keychain_read(&ulid))
        .await
        .map_err(|e| AiError::detailed("KEYCHAIN_DENIED", e.to_string(), e.to_string()))??;
    let pool = pool_for(&profile, &password)?;

    let version: String = timeout(
        Duration::from_secs(QUERY_TIMEOUT_SECS),
        sqlx::query_scalar(SQL_VERSION).fetch_one(&pool),
    )
    .await
    .map_err(|_| timed_out(QUERY_TIMEOUT_SECS))?
    .map_err(map_sqlx)?;

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
        };
        let mut b = DbProfile {
            host: "127.0.0.1".into(),
            ..a.clone()
        };
        assert_eq!(profile_sig(&a), profile_sig(&b));
        b.username = "rw".into();
        assert_ne!(profile_sig(&a), profile_sig(&b));
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
        };
        assert_eq!(
            connect_options(&p, "pw").unwrap_err().code,
            "DB_UNSUPPORTED"
        );
        let q = DbProfile {
            kind: "mysql".into(),
            host: "  ".into(),
            ..p
        };
        assert_eq!(
            connect_options(&q, "pw").unwrap_err().code,
            "DB_EMPTY_FIELD"
        );
    }

    #[test]
    fn keychain_account_is_namespaced_per_connection() {
        assert_eq!(keychain_account("01J"), "db/01J");
    }
}
