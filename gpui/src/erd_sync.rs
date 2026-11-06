// ERD 동기화 — 지금 앱 ErdView.tsx 의 syncOne, syncMany, autoSyncMissing 과 dbconn.ts 의 syncSchema.
// DB 를 읽는 건 amber-core 의 db_introspect(tokio 위)다. 여기는 스냅샷 마무리(지문), schema.json 쓰기, 연결 행의
// 마지막 동기화와 오류, 바뀐 것 띠를 맡는다. 시험은 DB 대신 fake_introspect 를 꽂는다(DB, 키체인에 닿지 않게).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use amber_core::ai::AiError;
use amber_gpui::dbconn::{DbConnection, enabled_schemas, schema_folder, snapshot_path};
use amber_gpui::i18n::t_with;
use amber_gpui::schema_snapshot::{
    RawSnapshot, diff_snapshots, finalize_snapshot, parse_snapshot, snapshot_text,
};
use futures::channel::oneshot;
use gpui_kit::*;

use crate::Spike;

/// 한 연결의 스키마를 동시에 읽는 수
const SYNC_WORKERS: usize = 3;

/// DB 대신 스키마 이름으로 RawSnapshot 을 주는 것(시험, 찍기)
pub type FakeIntrospect = Arc<dyn Fn(&str) -> Result<RawSnapshot, AiError> + Send + Sync>;

impl Spike {
    fn erd_tree_files(&self) -> amber_gpui::vault::Tree<'_> {
        amber_gpui::vault::Tree::new(&self.db, &amber_gpui::vault::ERD)
    }

    /// 스키마 하나를 DB 에서 다시 읽는다. 같은 스키마가 이미 도는 중이면 겹쳐 돌리지 않는다(None).
    /// 끝나면 성공했는지를 돌려준다 — syncMany 가 접속 실패면 남은 스키마를 건너뛴다
    pub(crate) fn erd_sync_one(
        &mut self,
        conn: &DbConnection,
        schema: &str,
        cx: &mut Context<Self>,
    ) -> Option<Task<bool>> {
        let folder = schema_folder(conn, schema);
        if self.erd.syncing.contains(&folder) {
            return None;
        }
        self.erd.syncing.insert(folder);
        self.erd.error = None;
        // 앞의 스냅샷은 읽기 전에 — 비교의 기준
        let previous = self
            .erd_tree_files()
            .read_file_if_exists(&snapshot_path(conn, schema))
            .ok()
            .flatten()
            .and_then(|body| parse_snapshot(&body));
        let rx = match self.erd.fake_introspect.clone() {
            Some(fake) => {
                let (tx, rx) = oneshot::channel();
                let _ = tx.send(fake(schema));
                rx
            }
            None => {
                let profile = amber_core::dbconn::DbProfile {
                    ulid: conn.ulid.clone(),
                    kind: conn.kind.clone(),
                    host: conn.host.clone(),
                    port: conn.port as u16,
                    username: conn.username.clone(),
                    tls: conn.tls.clone(),
                    credential_command: conn.credential_command.clone(),
                };
                let fut = amber_core::dbconn::db_introspect(profile, schema.to_string());
                crate::ai::spawn(async move {
                    // 앱의 형태로(지문은 여기서 붙인다)
                    fut.await.and_then(|raw| {
                        serde_json::to_value(raw)
                            .and_then(serde_json::from_value::<RawSnapshot>)
                            .map_err(|e| AiError::new("DB_QUERY", e.to_string()))
                    })
                })
            }
        };
        let (conn, schema) = (conn.clone(), schema.to_string());
        Some(cx.spawn(async move |this, cx| {
            let r = crate::ai::wait(rx, cx.background_executor().clone()).await;
            this.update(cx, |s, cx| {
                let ok = s.erd_sync_done(&conn, &schema, previous, r);
                cx.notify();
                ok
            })
            .unwrap_or(false)
        }))
    }

    /// syncSchema 의 뒷부분 — 쓰고, 표시하고, 비교한다. 실패는 연결 행에 코드로 남긴다
    fn erd_sync_done(
        &mut self,
        conn: &DbConnection,
        schema: &str,
        previous: Option<amber_gpui::erd_gen::SchemaSnapshot>,
        r: Option<Result<RawSnapshot, AiError>>,
    ) -> bool {
        let folder = schema_folder(conn, schema);
        self.erd.syncing.remove(&folder);
        let written = match r {
            Some(Ok(raw)) => {
                let snap = finalize_snapshot(raw);
                let files = self.erd_tree_files();
                files
                    .ensure_dir(&folder)
                    .and_then(|_| {
                        files.write_file(&snapshot_path(conn, schema), &snapshot_text(&snap))
                    })
                    .map(|_| snap)
                    .map_err(|e| AiError::new("", e.to_string()))
            }
            Some(Err(e)) => Err(e),
            // 버려졌다(화면이 닫혔다)
            None => return false,
        };
        let ok = match written {
            Ok(snap) => {
                let _ = amber_gpui::dbconn::mark_sync(&self.db, conn.id, None);
                if let Some(d) = previous.map(|p| diff_snapshots(&p, &snap))
                    && !d.is_empty()
                {
                    self.erd.diffs.insert(folder.clone(), d);
                }
                self.erd.snaps.insert(folder, Some(Arc::new(snap)));
                true
            }
            Err(e) => {
                let code = if e.code.is_empty() {
                    "DB_QUERY"
                } else {
                    e.code.as_str()
                };
                let _ = amber_gpui::dbconn::mark_sync(&self.db, conn.id, Some(code));
                self.erd.error = Some(t_with(
                    "erd.db.syncFail",
                    &[("msg", &crate::ai::err_text(&e))],
                ));
                false
            }
        };
        self.erd_refresh_rows();
        ok
    }

    /// 연결 행(마지막 동기화, 오류)만 다시 읽는다 — 스냅샷은 동기화가 이미 넣었다
    fn erd_refresh_rows(&mut self) {
        if let Ok(rows) = amber_gpui::dbconn::list_connections(&self.db) {
            self.erd.connections = Some(
                rows.into_iter()
                    .filter_map(|r| serde_json::from_value(r).ok())
                    .collect(),
            );
        }
    }

    /// 연결 하나의 스키마들을 셋씩 읽는다. 접속, 키체인 실패는 연결 단위의 사고라 하나라도 실패하면 남은 것은 건너뛴다
    pub(crate) fn erd_sync_many(
        &mut self,
        conn: &DbConnection,
        todo: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        if todo.is_empty() {
            return Task::ready(());
        }
        let folders: Vec<String> = todo.iter().map(|s| schema_folder(conn, s)).collect();
        self.erd.queued.extend(folders.iter().cloned());
        self.erd.expanded.insert(format!("c{}", conn.id));
        let queue = Rc::new(RefCell::new(VecDeque::from(todo)));
        let failed = Rc::new(Cell::new(false));
        let workers: Vec<Task<()>> = (0..SYNC_WORKERS)
            .map(|_| {
                let (queue, failed, conn) = (queue.clone(), failed.clone(), conn.clone());
                cx.spawn(async move |this, cx| {
                    loop {
                        if failed.get() {
                            return;
                        }
                        let Some(schema) = queue.borrow_mut().pop_front() else {
                            return;
                        };
                        let task = this
                            .update(cx, |s, cx| {
                                s.erd.queued.remove(&schema_folder(&conn, &schema));
                                cx.notify();
                                // 사용자가 먼저 눌러 이미 도는 중이다 — 기다리지 않고 넘어간다(실패로 세지 않는다)
                                s.erd_sync_one(&conn, &schema, cx)
                            })
                            .ok()
                            .flatten();
                        if let Some(task) = task
                            && !task.await
                        {
                            failed.set(true);
                        }
                    }
                })
            })
            .collect();
        cx.spawn(async move |this, cx| {
            futures::future::join_all(workers).await;
            let _ = this.update(cx, |s, cx| {
                for f in &folders {
                    s.erd.queued.remove(f);
                }
                cx.notify();
            });
        })
    }

    /// 스냅샷이 없는 스키마를 이번 실행에 한 번 채운다 — 새 연결의 첫 동기화. 연결마다 차례로
    pub(crate) fn erd_auto_sync(&mut self, cx: &mut Context<Self>) {
        let Some(list) = self.erd.connections.clone() else {
            return;
        };
        let mut jobs: Vec<(DbConnection, Vec<String>)> = Vec::new();
        for c in list {
            let todo: Vec<String> = enabled_schemas(&c)
                .into_iter()
                .map(|p| p.name.clone())
                .filter(|s| {
                    let folder = schema_folder(&c, s);
                    !self.erd.snaps.get(&folder).is_some_and(|x| x.is_some())
                        && !self.erd.attempted.contains(&folder)
                })
                .collect();
            for s in &todo {
                self.erd.attempted.insert(schema_folder(&c, s));
            }
            if !todo.is_empty() {
                jobs.push((c, todo));
            }
        }
        if jobs.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            for (c, todo) in jobs {
                let Ok(task) = this.update(cx, |s, cx| s.erd_sync_many(&c, todo, cx)) else {
                    return;
                };
                task.await;
            }
        })
        .detach();
    }

    /// 탭에 들어올 때마다 — 연결과 스냅샷을 다시 읽고, 없는 것을 채운다
    pub(crate) fn erd_enter(&mut self, cx: &mut Context<Self>) {
        self.load_erd();
        self.erd_auto_sync(cx);
    }

    /// 즐겨찾기 — 트리 순서가 바로 바뀌도록 화면 상태를 먼저 고치고 저장한다. 그 뒤 행을 다시 읽는다
    pub(crate) fn erd_toggle_star(&mut self, conn_id: i64, schema: &str) {
        let Some(list) = self.erd.connections.as_mut() else {
            return;
        };
        let Some(c) = list.iter_mut().find(|c| c.id == conn_id) else {
            return;
        };
        let Some(p) = c.schemas.iter_mut().find(|p| p.name == schema) else {
            return;
        };
        let starred = p.starred != Some(true);
        p.starred = Some(starred);
        // 저장된 행 그대로(모르는 키까지) 고쳐 쓴다 — 형태로 바꿨다 되돌리면 모르는 키가 빠진다
        let saved = amber_gpui::dbconn::get_connection(&self.db, conn_id).and_then(|row| match row
            .as_ref()
            .and_then(|r| r.as_object())
        {
            Some(o) => amber_gpui::dbconn::set_schema_starred(&self.db, o, schema, starred),
            None => Ok(()),
        });
        if let Err(e) = saved {
            self.erd.error = Some(e.to_string());
        }
        self.erd_refresh_rows();
    }
}
