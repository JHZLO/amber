// src/lib/timeBlocks.ts 의 DB 함수를 옮긴 것. SQL 은 그대로다($n → ?n 만).

use crate::db::{Db, Result, Row, int, opt_int, text};

pub fn list_blocks(db: &Db, from: &str, to: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT * FROM time_blocks WHERE date BETWEEN ?1 AND ?2 ORDER BY date, start_min, id",
        &[text(from), text(to)],
    )
}

pub fn create_block(
    db: &Db,
    date: &str,
    start_min: i64,
    end_min: i64,
    title: &str,
    todo_id: Option<i64>,
) -> Result<i64> {
    let (_, id) = db.execute(
        "INSERT INTO time_blocks (date, start_min, end_min, title, todo_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        &[
            text(date),
            int(start_min),
            int(end_min),
            text(title),
            opt_int(todo_id),
        ],
    )?;
    Ok(id)
}

pub fn update_block_time(db: &Db, id: i64, date: &str, start_min: i64, end_min: i64) -> Result<()> {
    db.execute(
        "UPDATE time_blocks SET date = ?1, start_min = ?2, end_min = ?3, updated_at = ?4 WHERE id = ?5",
        &[text(date), int(start_min), int(end_min), int(db.now()), int(id)],
    )?;
    Ok(())
}

pub fn rename_block(db: &Db, id: i64, title: &str) -> Result<()> {
    db.execute(
        "UPDATE time_blocks SET title = ?1, updated_at = ?2 WHERE id = ?3",
        &[text(title), int(db.now()), int(id)],
    )?;
    Ok(())
}

pub fn delete_block(db: &Db, id: i64) -> Result<()> {
    db.execute("DELETE FROM time_blocks WHERE id = ?1", &[int(id)])?;
    Ok(())
}
