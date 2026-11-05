// src/lib/vacations.ts 의 DB 함수를 옮긴 것. SQL 은 그대로다($n → ?n 만).

use serde_json::{Map, Value as Json};

use crate::db::{Db, Result, int, text};

const KINDS: [&str; 5] = ["annual", "half", "sick", "public", "special"];

/// 모르는 코드(예전 버전이 넣었거나 손으로 고친 값)는 기본값으로 읽는다
fn kind_of(raw: Option<&str>) -> String {
    raw.filter(|k| KINDS.contains(k))
        .unwrap_or(KINDS[0])
        .to_string()
}

/// 'YYYY-MM-DD' → 종류
pub fn list_vacations(db: &Db, from: &str, to: &str) -> Result<Map<String, Json>> {
    let rows = db.select(
        "SELECT date, kind FROM vacations WHERE date BETWEEN ?1 AND ?2",
        &[text(from), text(to)],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let date = r.get("date")?.as_str()?.to_string();
            Some((date, Json::from(kind_of(r.get("kind")?.as_str()))))
        })
        .collect())
}

pub fn get_vacation(db: &Db, date: &str) -> Result<Option<String>> {
    let rows = db.select("SELECT kind FROM vacations WHERE date = ?1", &[text(date)])?;
    Ok(rows
        .first()
        .map(|r| kind_of(r.get("kind").and_then(Json::as_str))))
}

pub fn set_vacation(db: &Db, date: &str, kind: &str) -> Result<()> {
    db.execute(
        "INSERT INTO vacations (date, kind) VALUES (?1, ?2)
     ON CONFLICT(date) DO UPDATE SET kind = ?3, updated_at = ?4",
        &[text(date), text(kind), text(kind), int(db.now())],
    )?;
    Ok(())
}

pub fn remove_vacation(db: &Db, date: &str) -> Result<()> {
    db.execute("DELETE FROM vacations WHERE date = ?1", &[text(date)])?;
    Ok(())
}
