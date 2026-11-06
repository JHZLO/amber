// ERD 동기화 — 지금 앱 ErdView.tsx 의 syncOne, syncMany, autoSyncMissing 과 dbconn.ts 의 syncSchema.
// DB 를 읽는 건 amber-core 의 db_introspect(tokio 위)다. 여기는 스냅샷 마무리(지문), schema.json 쓰기, 연결 행의
// 마지막 동기화와 오류, 바뀐 것 띠를 맡는다. DB 와 키체인은 DbBackend 를 거친다 — 앱은 amber-core, 시험과 찍기는
// 가짜(Refuse 가 기본)를 꽂아 DB, 키체인에 닿지 않는다.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use amber_core::ai::AiError;
use amber_core::dbconn::{DbProfile, DbTestResult};
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
/// 옛 ERD 정리 띠를 다시 묻지 않는다
const LEGACY_DISMISSED_KEY: &str = "amber.erd.legacy-dismissed";

type Rx<T> = oneshot::Receiver<Result<T, AiError>>;

/// DB 와 키체인에 하는 일 전부 — 결과는 tokio 쪽에서 오고 crate::ai::wait 로 기다린다
pub trait DbBackend {
    fn introspect(&self, profile: DbProfile, schema: String) -> Rx<RawSnapshot>;
    fn test(
        &self,
        profile: DbProfile,
        password: Option<String>,
        schemas: Option<Vec<String>>,
    ) -> Rx<DbTestResult>;
    fn secret_set(&self, ulid: String, password: String) -> Rx<()>;
    fn secret_delete(&self, ulid: String) -> Rx<()>;
}

/// 앱 — amber-core(MySQL, macOS 키체인)
pub struct CoreDb;

impl DbBackend for CoreDb {
    fn introspect(&self, profile: DbProfile, schema: String) -> Rx<RawSnapshot> {
        let fut = amber_core::dbconn::db_introspect(profile, schema);
        crate::ai::spawn(async move {
            // 앱의 형태로(지문은 여기서 붙인다)
            fut.await.and_then(|raw| {
                serde_json::to_value(raw)
                    .and_then(serde_json::from_value::<RawSnapshot>)
                    .map_err(|e| AiError::new("DB_QUERY", e.to_string()))
            })
        })
    }
    fn test(
        &self,
        profile: DbProfile,
        password: Option<String>,
        schemas: Option<Vec<String>>,
    ) -> Rx<DbTestResult> {
        crate::ai::spawn(amber_core::dbconn::db_test(profile, password, schemas))
    }
    fn secret_set(&self, ulid: String, password: String) -> Rx<()> {
        crate::ai::spawn(amber_core::dbconn::db_secret_set(ulid, password))
    }
    fn secret_delete(&self, ulid: String) -> Rx<()> {
        crate::ai::spawn(amber_core::dbconn::db_secret_delete(ulid))
    }
}

/// 이미 끝난 결과
pub fn ready<T>(v: Result<T, AiError>) -> Rx<T> {
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(v);
    rx
}

/// 시험과 찍기의 기본 — 무엇이든 거절한다(DB 가 없다)
pub struct Refuse;

impl DbBackend for Refuse {
    fn introspect(&self, _: DbProfile, _: String) -> Rx<RawSnapshot> {
        ready(Err(AiError::new("DB_REFUSED", "")))
    }
    fn test(&self, _: DbProfile, _: Option<String>, _: Option<Vec<String>>) -> Rx<DbTestResult> {
        ready(Err(AiError::new("DB_REFUSED", "")))
    }
    fn secret_set(&self, _: String, _: String) -> Rx<()> {
        ready(Err(AiError::new("DB_CREDENTIAL", "")))
    }
    fn secret_delete(&self, _: String) -> Rx<()> {
        ready(Ok(()))
    }
}

/// 연결 행 → 접속 프로필
pub fn profile_of(c: &DbConnection) -> DbProfile {
    DbProfile {
        ulid: c.ulid.clone(),
        kind: c.kind.clone(),
        host: c.host.clone(),
        port: c.port as u16,
        username: c.username.clone(),
        tls: c.tls.clone(),
        credential_command: c.credential_command.clone(),
    }
}

impl Spike {
    /// 꽂힌 DB(시험, 찍기) 또는 amber-core
    pub(crate) fn erd_db(&self) -> Rc<dyn DbBackend> {
        self.erd.db.clone().unwrap_or_else(|| Rc::new(CoreDb))
    }

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
        let rx = self
            .erd_db()
            .introspect(profile_of(conn), schema.to_string());
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

    /// 탭에 들어올 때마다 — 연결과 스냅샷을 다시 읽고, 없는 것을 채운다. 처음 한 번은 옛 다이어그램 탭의 스냅샷을
    /// 옮겨 온다(복사 — 다시 읽지 않아도 지난 구조가 바로 보이게). 남은 자동 생성물은 띠로 묻는다
    pub(crate) fn erd_enter(&mut self, cx: &mut Context<Self>) {
        if !self.erd.migrated {
            self.erd.migrated = true;
            let list: Vec<DbConnection> = amber_gpui::dbconn::list_connections(&self.db)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|r| serde_json::from_value(r).ok())
                .collect();
            let _ = amber_gpui::erd_migrate::copy_legacy_snapshots(&self.db, &list);
            if amber_gpui::prefs::get(&self.db, LEGACY_DISMISSED_KEY).as_deref() != Some("1") {
                self.erd.legacy = amber_gpui::erd_migrate::find_legacy_erds(&self.db, &list);
            }
        }
        self.load_erd();
        self.erd_auto_sync(cx);
    }

    /// 연결 끊기 — 프로필, 키체인 비밀번호, 이 연결의 스냅샷(휴지통). DB 에는 아무 일도 없다
    pub(crate) fn erd_disconnect(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.erd.confirm_disconnect.clone() else {
            return;
        };
        if self.erd.busy {
            return;
        }
        self.erd.busy = true;
        let task = self.erd_delete_connection(&c, cx);
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |s, cx| {
                s.erd.busy = false;
                s.erd.confirm_disconnect = None;
                match r {
                    Ok(()) => {
                        if s.erd.selected.as_ref().is_some_and(|(u, _)| *u == c.ulid) {
                            s.erd.selected = None;
                            amber_gpui::prefs::set(&s.db, "amber.erd.selected", "null");
                        }
                        // 같은 이름으로 다시 연결하면 같은 폴더를 쓴다 — 지난 시도와 바뀐 것 기록을 비워 첫 동기화가 다시 돌게
                        let prefix = format!("{}/", c.folder_path);
                        s.erd.attempted.retain(|f| !f.starts_with(&prefix));
                        s.erd.diffs.retain(|f, _| !f.starts_with(&prefix));
                        s.load_erd();
                    }
                    Err(e) => s.erd.error = Some(crate::ai::err_text(&e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 옛 ERD 정리 — 확인을 누른 지금 다시 본다(띠가 떠 있는 사이 그 폴더에 파일을 만들었을 수 있다)
    pub(crate) fn erd_trash_legacy(&mut self) {
        if self.erd.legacy.is_none() {
            return;
        }
        let list = self.erd.connections.clone().unwrap_or_default();
        let r = match amber_gpui::erd_migrate::find_legacy_erds(&self.db, &list) {
            Some(plan) => amber_gpui::erd_migrate::trash_legacy_erds(&self.db, &plan),
            None => Ok(()),
        };
        match r {
            Ok(()) => self.erd.legacy = None,
            Err(e) => self.erd.error = Some(e),
        }
        self.erd.confirm_legacy = false;
    }

    /// 옛 ERD 를 남겨 둔다 — 다시 묻지 않는다
    pub(crate) fn erd_keep_legacy(&mut self) {
        amber_gpui::prefs::set(&self.db, LEGACY_DISMISSED_KEY, "1");
        self.erd.legacy = None;
    }

    /// 연결 삭제(deleteConnection) — 키체인 항목, 행, 스냅샷 폴더(휴지통). 키체인을 못 지우면 멈춘다.
    /// 한 번도 동기화하지 않은 연결은 폴더가 없다(폴더 실패는 넘긴다)
    pub(crate) fn erd_delete_connection(
        &mut self,
        conn: &DbConnection,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), AiError>> {
        let rx = self.erd_db().secret_delete(conn.ulid.clone());
        let conn = conn.clone();
        cx.spawn(async move |this, cx| {
            let r = crate::ai::wait(rx, cx.background_executor().clone())
                .await
                .unwrap_or_else(|| Err(AiError::new("", "dropped")));
            r?;
            this.update(cx, |s, _| {
                amber_gpui::dbconn::delete_connection_row(&s.db, conn.id)
                    .map_err(|e| AiError::new("", e.to_string()))?;
                let _ = s.erd_tree_files().delete_entry(&conn.folder_path);
                Ok(())
            })
            .unwrap_or_else(|_| Err(AiError::new("", "dropped")))
        })
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
