// 지금 앱(tauri-plugin-sql 2.4 = sqlx 0.8 Migrator)과 같은 방식으로 마이그레이션을 깐다. 장부(_sqlx_migrations)와
// 체크섬(파일 전체의 SHA-384)이 같아야 두 앱이 같은 amber.db 를 번갈아 열어도 서로를 망가뜨리지 않는다.
// 지금 앱과 다른 점 하나: 지금 앱은 장부가 어긋나면 로그만 남기고 조용히 뜬다(새 컬럼만 없다).
// 여기서는 오류로 돌려준다.

use sha2::{Digest, Sha384};

use crate::db::Db;

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

pub fn checksum(sql: &str) -> Vec<u8> {
    Sha384::digest(sql.as_bytes()).to_vec()
}

pub fn run(db: &Db) -> Result<(), String> {
    let conn = &db.conn;
    let sql_err = |e: rusqlite::Error| e.to_string();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);",
    )
    .map_err(sql_err)?;

    let dirty: Option<i64> = conn
        .query_row(
            "SELECT version FROM _sqlx_migrations WHERE success = false ORDER BY version LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(v) = dirty {
        return Err(format!("마이그레이션 {v} 이 실패한 채 남아 있다"));
    }

    let mut stmt = conn
        .prepare("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .map_err(sql_err)?;
    let applied: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql_err)?
        .collect::<Result<_, _>>()
        .map_err(sql_err)?;
    drop(stmt);

    for (v, _) in &applied {
        if !MIGRATIONS.iter().any(|(m, ..)| m == v) {
            return Err(format!("DB 에 깔린 마이그레이션 {v} 을 이 앱이 모른다"));
        }
    }
    for (version, description, sql) in MIGRATIONS {
        let sum = checksum(sql);
        match applied.iter().find(|(v, _)| v == version) {
            Some((_, have)) if *have != sum => {
                return Err(format!(
                    "마이그레이션 {version} 의 체크섬이 다르다 — 커밋된 파일이 바뀌었다"
                ));
            }
            Some(_) => {}
            None => {
                let started = std::time::Instant::now();
                let tx = conn.unchecked_transaction().map_err(sql_err)?;
                tx.execute_batch(sql)
                    .map_err(|e| format!("마이그레이션 {version}: {e}"))?;
                tx.execute(
                    "INSERT INTO _sqlx_migrations ( version, description, success, checksum, execution_time )
    VALUES ( ?1, ?2, TRUE, ?3, -1 )",
                    rusqlite::params![version, description, sum],
                )
                .map_err(sql_err)?;
                tx.commit().map_err(sql_err)?;
                conn.execute(
                    "UPDATE _sqlx_migrations SET execution_time = ?1 WHERE version = ?2",
                    rusqlite::params![started.elapsed().as_nanos() as i64, version],
                )
                .map_err(sql_err)?;
            }
        }
    }
    Ok(())
}
