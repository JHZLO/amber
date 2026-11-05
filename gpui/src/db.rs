// SQLite 연결과 행 모양. 지금 앱(tauri-plugin-sql)이 프런트에 넘기던 모양 그대로 행을 JSON 객체로
// 돌려준다 — INTEGER 는 정수, REAL 은 실수, TEXT 는 문자열, NULL 은 null. 그래야 동등성 하네스의
// 정답(parity/fixtures)과 한 글자도 다르지 않게 비교된다.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use rusqlite::functions::FunctionFlags;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, params_from_iter};
use serde_json::{Map, Value as Json};

pub type Row = Map<String, Json>;
pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// 앱은 벽시계, 하네스는 단계마다 정해 주는 시각을 쓴다
#[derive(Clone)]
pub enum Clock {
    System,
    Fixed(Arc<AtomicI64>),
}

pub struct Db {
    pub conn: Connection,
    clock: Clock,
    /// 하네스에서 이 연결이 낸 ULID 수
    ulids: AtomicU64,
    /// 하네스에서 이 연결이 휴지통에 넣은 수
    trashed: AtomicU64,
}

impl Db {
    pub fn open(path: &str, clock: Clock) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        if let Clock::Fixed(now) = &clock {
            // 기본값과 트리거가 쓰는 unixepoch('subsec') 도 하네스 시계를 본다
            let now = now.clone();
            conn.create_scalar_function("unixepoch", -1, FunctionFlags::SQLITE_UTF8, move |ctx| {
                if ctx.len() == 1 && ctx.get::<String>(0).as_deref() == Ok("subsec") {
                    Ok(now.load(Ordering::SeqCst) as f64 / 1000.0)
                } else {
                    Err(rusqlite::Error::UserFunctionError(
                        "unixepoch: 하네스 시계에 묶이지 않은 호출".into(),
                    ))
                }
            })?;
        }
        Ok(Db {
            conn,
            clock,
            ulids: AtomicU64::new(0),
            trashed: AtomicU64::new(0),
        })
    }

    pub fn now(&self) -> i64 {
        match &self.clock {
            Clock::System => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            Clock::Fixed(now) => now.load(Ordering::SeqCst),
        }
    }

    /// 앱은 진짜 ULID, 하네스는 연결을 열 때마다 parityUlid(1), (2)… 순서(parity/sqlite.ts 의 엔진 계약)
    pub fn new_ulid(&self) -> String {
        match &self.clock {
            Clock::System => ulid::Ulid::new().to_string(),
            Clock::Fixed(_) => {
                const ALPHA: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
                let mut n = self.ulids.fetch_add(1, Ordering::SeqCst) + 1;
                let mut digits = [b'0'; 24];
                for d in digits.iter_mut().rev() {
                    *d = ALPHA[(n % 32) as usize];
                    n /= 32;
                }
                format!("01{}", String::from_utf8_lossy(&digits))
            }
        }
    }

    /// 앱 데이터 폴더 = DB 파일이 있는 폴더(vault/ 가 옆에 있다)
    pub fn data_dir(&self) -> PathBuf {
        self.conn
            .path()
            .and_then(|p| Path::new(p).parent())
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// 앱 데이터 폴더 기준 상대경로 또는 절대경로 → 실제 경로
    pub fn at(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.data_dir().join(path)
        }
    }

    /// 휴지통으로(없으면 그대로 성공). 앱은 macOS 휴지통, 하네스는 `.trash/<n>/<이름>`(parity/fs.ts 의 엔진 계약)
    pub fn trash(&self, p: &str) -> std::result::Result<(), String> {
        match &self.clock {
            Clock::System => {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default();
                amber_core::files::move_to_trash(&self.data_dir(), &home, p).map_err(|e| e.message)
            }
            Clock::Fixed(_) => {
                let from = self.at(p);
                if !from.exists() {
                    return Ok(());
                }
                let n = self.trashed.fetch_add(1, Ordering::SeqCst) + 1;
                let dir = self.data_dir().join(".trash").join(n.to_string());
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let name = from.file_name().unwrap_or_default();
                std::fs::rename(&from, dir.join(name)).map_err(|e| e.to_string())
            }
        }
    }

    pub fn select(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        let mut stmt = self.conn.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map(params_from_iter(params.iter()), |r| {
            let mut row = Row::new();
            for (i, name) in names.iter().enumerate() {
                row.insert(name.clone(), json_of(r.get_ref(i)?));
            }
            Ok(row)
        })?;
        rows.collect()
    }

    /// (바뀐 행 수, 마지막 rowid)
    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<(usize, i64)> {
        let n = self.conn.execute(sql, params_from_iter(params.iter()))?;
        Ok((n, self.conn.last_insert_rowid()))
    }
}

fn json_of(v: ValueRef<'_>) -> Json {
    match v {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(i) => Json::from(i),
        ValueRef::Real(f) => Json::from(f),
        ValueRef::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Json::from(b.to_vec()),
    }
}

pub fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

pub fn int(i: i64) -> Value {
    Value::Integer(i)
}

pub fn opt_int(i: Option<i64>) -> Value {
    i.map_or(Value::Null, Value::Integer)
}

pub fn get_i64(row: &Row, key: &str) -> Option<i64> {
    row.get(key).and_then(Json::as_i64)
}
