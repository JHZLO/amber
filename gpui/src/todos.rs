// src/lib/todos.ts 를 옮긴 것. SQL 은 한 글자도 바꾸지 않았다($n → ?n 만). 규칙의 이유는 원본 주석에 있다 —
// 여기서는 다시 적지 않는다. 동작이 같은지는 parity/ 하네스가 잰다(AMBER_PARITY_ENGINE).

use std::collections::{HashMap, HashSet};

use rusqlite::types::Value;

use crate::db::{Db, Result, Row, get_i64, int, opt_int, text};

/// 서브트리 재귀의 경계 — todos.ts parkedBoundary
fn parked_boundary(seed: &str) -> String {
    format!("(t.parked_at IS NULL) = (SELECT parked_at IS NULL FROM todos WHERE id = {seed})")
}

pub fn list_todos(db: &Db, date: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT t.id, t.content, t.due_date, t.scope, t.done, t.completed_at, t.parent_id,
                t.sort_order, t.created_at, t.updated_at, t.parked_at,
                0 AS carried, 0 AS gone
           FROM todos t
          WHERE t.due_date = ?1 AND t.scope = 'day' AND t.parked_at IS NULL
         UNION ALL
         SELECT c.todo_id AS id, c.content, COALESCE(t.due_date, c.date) AS due_date,
                'day' AS scope, COALESCE(t.done, c.done) AS done, t.completed_at,
                c.parent_id, COALESCE(t.sort_order, 0) AS sort_order,
                COALESCE(t.created_at, 0) AS created_at, COALESCE(t.updated_at, 0) AS updated_at,
                NULL AS parked_at,
                1 AS carried, CASE WHEN t.id IS NULL THEN 1 ELSE 0 END AS gone
           FROM todo_carries c
           LEFT JOIN todos t ON t.id = c.todo_id AND t.scope = 'day' AND t.parked_at IS NULL
          WHERE c.date = ?2 AND (t.id IS NULL OR t.due_date <> ?3)
         ORDER BY carried, gone, sort_order, id",
        &[text(date), text(date), text(date)],
    )
}

pub fn list_range_rows(db: &Db, from: &str, to: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT t.due_date AS day, t.id, t.parent_id, t.done, 0 AS gone
           FROM todos t
          WHERE t.due_date BETWEEN ?1 AND ?2 AND t.scope = 'day' AND t.parked_at IS NULL
         UNION ALL
         SELECT c.date AS day, c.todo_id AS id, c.parent_id, COALESCE(t.done, c.done) AS done,
                CASE WHEN t.id IS NULL THEN 1 ELSE 0 END AS gone
           FROM todo_carries c
           LEFT JOIN todos t ON t.id = c.todo_id AND t.scope = 'day' AND t.parked_at IS NULL
          WHERE c.date BETWEEN ?3 AND ?4 AND (t.id IS NULL OR t.due_date <> c.date)",
        &[text(from), text(to), text(from), text(to)],
    )
}

pub fn list_overdue_open(db: &Db, before: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT * FROM todos
          WHERE done = 0 AND due_date < ?1 AND scope = 'day' AND parked_at IS NULL
          ORDER BY due_date, sort_order, id",
        &[text(before)],
    )
}

pub fn create_todo(
    db: &Db,
    content: &str,
    due_date: &str,
    parent_id: Option<i64>,
    scope: &str,
) -> Result<i64> {
    let (_, id) = db.execute(
        "INSERT INTO todos (content, due_date, parent_id, sort_order, scope)
         VALUES (?1, ?2, ?3,
           (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM todos
             WHERE due_date = ?4 AND parent_id IS ?5 AND scope = ?6), ?6)",
        &[
            text(content),
            text(due_date),
            opt_int(parent_id),
            text(due_date),
            opt_int(parent_id),
            text(scope),
        ],
    )?;
    Ok(id)
}

pub fn list_week_todos(db: &Db, monday: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT *, 0 AS carried FROM todos
          WHERE due_date = ?1 AND scope = 'week' AND parked_at IS NULL
          ORDER BY sort_order, id",
        &[text(monday)],
    )
}

pub fn list_week_counts(db: &Db, from: &str, to: &str) -> Result<Vec<Row>> {
    db.select(
        "SELECT due_date, COUNT(*) AS total, COALESCE(SUM(done), 0) AS done
           FROM todos
          WHERE due_date BETWEEN ?1 AND ?2 AND scope = 'week' AND parked_at IS NULL
          GROUP BY due_date",
        &[text(from), text(to)],
    )
}

pub fn list_parked(db: &Db) -> Result<Vec<Row>> {
    db.select(
        "SELECT *, 0 AS carried, 0 AS gone FROM todos
          WHERE parked_at IS NOT NULL
          ORDER BY parked_at, sort_order, id",
        &[],
    )
}

pub fn park_subtree(db: &Db, id: i64) -> Result<()> {
    db.execute(
        "WITH RECURSIVE sub(id) AS (
           SELECT ?1
           UNION
           SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
         )
         UPDATE todos
            SET parked_at = CAST(unixepoch('subsec') * 1000 AS INTEGER),
                updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
          WHERE parked_at IS NULL AND id IN (SELECT id FROM sub)",
        &[int(id)],
    )?;
    Ok(())
}

pub fn list_parked_ancestors(db: &Db) -> Result<Vec<Row>> {
    db.select(
        "WITH RECURSIVE anc(id, content, parent_id) AS (
           SELECT t.id, t.content, t.parent_id
             FROM todos t
            WHERE t.id IN (SELECT parent_id FROM todos
                            WHERE parked_at IS NOT NULL AND parent_id IS NOT NULL)
           UNION
           SELECT t.id, t.content, t.parent_id
             FROM todos t JOIN anc ON t.id = anc.parent_id
         )
         SELECT id, content, parent_id FROM anc",
        &[],
    )
}

pub fn unpark_subtree(db: &Db, id: i64, due_date: &str) -> Result<Option<i64>> {
    let rows = db.select("SELECT parent_id FROM todos WHERE id = ?1", &[int(id)])?;
    let parent_id = match rows.first() {
        Some(row) => restore_chain(db, get_i64(row, "parent_id"), due_date)?,
        None => None,
    };
    db.execute(
        "WITH RECURSIVE sub(id) AS (
           SELECT ?2
           UNION
           SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
         )
         UPDATE todos
            SET parked_at = NULL,
                due_date = ?1,
                scope = 'day',
                updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
          WHERE parked_at IS NOT NULL AND id IN (SELECT id FROM sub)",
        &[text(due_date), int(id)],
    )?;
    db.execute(
        "UPDATE todos SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
        &[opt_int(parent_id), int(db.now()), int(id)],
    )?;
    Ok(parent_id)
}

fn restore_chain(db: &Db, parent_id: Option<i64>, due_date: &str) -> Result<Option<i64>> {
    let Some(mut cur) = parent_id else {
        return Ok(None);
    };
    let mut chain: Vec<(i64, String)> = Vec::new();
    let mut seen = HashSet::new();
    while seen.insert(cur) {
        let rows = db.select(
            "SELECT id, content, parent_id FROM todos WHERE id = ?1",
            &[int(cur)],
        )?;
        let Some(a) = rows.first() else { break };
        let content = a
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        chain.insert(0, (cur, content));
        match get_i64(a, "parent_id") {
            Some(p) => cur = p,
            None => break,
        }
    }
    let mut attach: Option<i64> = None;
    for (_, content) in chain {
        let hits = db.select(
            "SELECT id FROM todos
              WHERE due_date = ?1 AND scope = 'day' AND parked_at IS NULL
                AND content = ?2 AND parent_id IS ?3
              ORDER BY id LIMIT 1",
            &[text(due_date), text(&content), opt_int(attach)],
        )?;
        attach = Some(match hits.first().and_then(|h| get_i64(h, "id")) {
            Some(id) => id,
            None => create_todo(db, &content, due_date, attach, "day")?,
        });
    }
    Ok(attach)
}

pub fn reparent_todo(db: &Db, id: i64, parent_id: Option<i64>) -> Result<()> {
    db.execute(
        "UPDATE todos SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
        &[opt_int(parent_id), int(db.now()), int(id)],
    )?;
    Ok(())
}

pub fn reorder_todos(db: &Db, ordered_ids: &[i64]) -> Result<()> {
    for (i, id) in ordered_ids.iter().enumerate() {
        db.execute(
            "UPDATE todos SET sort_order = ?1 WHERE id = ?2",
            &[int(i as i64), int(*id)],
        )?;
    }
    Ok(())
}

pub fn set_subtree_done(db: &Db, id: i64, done: i64) -> Result<()> {
    db.execute(
        &format!(
            "WITH RECURSIVE sub(id) AS (
               SELECT ?2
               UNION
               SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
                WHERE {}
             )
             UPDATE todos SET done = ?1
              WHERE done <> ?1 AND id IN (SELECT id FROM sub)",
            parked_boundary("?2")
        ),
        &[int(done), int(id)],
    )?;
    Ok(())
}

pub fn recompute_parent_done(db: &Db, parent_id: i64) -> Result<()> {
    let rows = db.select(
        "SELECT COALESCE(SUM(done = 0), 0) AS open, COUNT(*) AS total
           FROM todos WHERE parent_id = ?1 AND parked_at IS NULL",
        &[int(parent_id)],
    )?;
    let (open, total) = rows
        .first()
        .map(|r| {
            (
                get_i64(r, "open").unwrap_or(0),
                get_i64(r, "total").unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));
    let done = if total > 0 && open == 0 { 1 } else { 0 };
    db.execute(
        "UPDATE todos SET done = ?1 WHERE id = ?2 AND done <> ?1",
        &[int(done), int(parent_id)],
    )?;
    Ok(())
}

pub fn recompute_chain_from(db: &Db, parent_id: Option<i64>) -> Result<()> {
    let mut pid = parent_id;
    while let Some(p) = pid {
        recompute_parent_done(db, p)?;
        let rows = db.select("SELECT parent_id FROM todos WHERE id = ?1", &[int(p)])?;
        pid = rows.first().and_then(|r| get_i64(r, "parent_id"));
    }
    Ok(())
}

pub fn update_todo_content(db: &Db, id: i64, content: &str) -> Result<()> {
    db.execute(
        "UPDATE todos SET content = ?1, updated_at = ?2 WHERE id = ?3",
        &[text(content), int(db.now()), int(id)],
    )?;
    Ok(())
}

fn placeholders(n: usize, from: usize) -> String {
    (0..n)
        .map(|i| format!("?{}", i + from))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn move_todos(db: &Db, ids: &[i64], due_date: &str) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let id_params: Vec<Value> = ids.iter().map(|&i| int(i)).collect();
    let all = db.select(
        &format!(
            "WITH RECURSIVE sub(id) AS (
               SELECT id FROM todos WHERE id IN ({}) AND parked_at IS NULL
               UNION SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
                WHERE t.parked_at IS NULL
             )
             SELECT * FROM todos
              WHERE id IN (SELECT id FROM sub)
              ORDER BY sort_order, id",
            placeholders(ids.len(), 1)
        ),
        &id_params,
    )?;
    if all.is_empty() {
        return Ok(());
    }
    let by_id: HashMap<i64, &Row> = all
        .iter()
        .filter_map(|t| Some((get_i64(t, "id")?, t)))
        .collect();
    let mut by_parent: HashMap<i64, Vec<&Row>> = HashMap::new();
    for t in &all {
        if let Some(p) = get_i64(t, "parent_id") {
            by_parent.entry(p).or_default().push(t);
        }
    }
    // 옮길 대상 = 씨앗 + 미완료 자식만 따라간 자손. 넣은 순서를 지킨다(TS 의 Set 과 같다)
    let mut moving: Vec<i64> = Vec::new();
    let mut seen = HashSet::new();
    fn walk(
        id: i64,
        by_parent: &HashMap<i64, Vec<&Row>>,
        moving: &mut Vec<i64>,
        seen: &mut HashSet<i64>,
    ) {
        if !seen.insert(id) {
            return;
        }
        moving.push(id);
        for c in by_parent.get(&id).map(Vec::as_slice).unwrap_or_default() {
            if get_i64(c, "done") == Some(0) {
                if let Some(cid) = get_i64(c, "id") {
                    walk(cid, by_parent, moving, seen);
                }
            }
        }
    }
    for id in ids {
        if by_id.contains_key(id) {
            walk(*id, &by_parent, &mut moving, &mut seen);
        }
    }
    let targets: Vec<i64> = moving
        .into_iter()
        .filter(|id| by_id[id].get("due_date").and_then(|v| v.as_str()) != Some(due_date))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let target_params: Vec<Value> = targets.iter().map(|&i| int(i)).collect();
    db.execute(
        &format!(
            "INSERT OR IGNORE INTO todo_carries (todo_id, date, content, parent_id, done)
             SELECT id, due_date, content, parent_id, done FROM todos
              WHERE id IN ({})",
            placeholders(targets.len(), 1)
        ),
        &target_params,
    )?;
    let mut params = vec![text(due_date), int(db.now())];
    params.extend(target_params);
    db.execute(
        &format!(
            "UPDATE todos SET due_date = ?1, updated_at = ?2
              WHERE id IN ({})",
            placeholders(targets.len(), 3)
        ),
        &params,
    )?;
    Ok(())
}

pub fn delete_todo(db: &Db, id: i64) -> Result<()> {
    let sub = format!(
        "WITH RECURSIVE sub(id) AS (
           SELECT ?1
           UNION
           SELECT t.id FROM todos t JOIN sub ON t.parent_id = sub.id
            WHERE {}
         )",
        parked_boundary("?1")
    );
    let p = [int(id)];
    db.execute(
        &format!(
            "{sub}
             UPDATE todo_carries
                SET done = COALESCE((SELECT done FROM todos WHERE id = todo_carries.todo_id), done)
              WHERE todo_id IN (SELECT id FROM sub)"
        ),
        &p,
    )?;
    db.execute(
        &format!(
            "{sub}
             UPDATE time_blocks
                SET title = COALESCE((SELECT content FROM todos WHERE id = time_blocks.todo_id), title)
              WHERE title = '' AND todo_id IN (SELECT id FROM sub)"
        ),
        &p,
    )?;
    db.execute(
        &format!(
            "{sub}
             DELETE FROM time_blocks
              WHERE todo_id IN (SELECT id FROM sub)
                AND NOT EXISTS (
                  SELECT 1 FROM todo_carries c
                   WHERE c.todo_id = time_blocks.todo_id AND c.date = time_blocks.date
                )"
        ),
        &p,
    )?;
    db.execute(
        &format!(
            "{sub}
             UPDATE todos
                SET parent_id = NULL, updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
              WHERE parked_at IS NOT NULL AND parent_id IN (SELECT id FROM sub)"
        ),
        &p,
    )?;
    db.execute(
        &format!("{sub} DELETE FROM todos WHERE id IN (SELECT id FROM sub)"),
        &p,
    )?;
    Ok(())
}

pub fn remove_carry(db: &Db, todo_id: i64, date: &str) -> Result<()> {
    db.execute(
        "DELETE FROM todo_carries WHERE todo_id = ?1 AND date = ?2",
        &[int(todo_id), text(date)],
    )?;
    Ok(())
}
