// 열린 노트를 디스크와 맞춘다 — 지금 앱 lib/noteWatch.ts. 다른 프로그램(에이전트, vim, git)이 파일을 고치면
// 노트를 다시 열지 않아도 화면에 반영된다. 감지는 두 겹이다: 보관함 폴더의 OS 파일 이벤트(FSEvents, 250ms 모아서)가
// 주력이고, 창으로 돌아올 때의 비교가 이벤트 유실(잠자기, 감시 실패)을 메운다.
// 내용 비교가 기준이다 — mtime 만 보면 방금 우리가 저장한 파일도 '바뀌었다'로 읽힌다.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use amber_gpui::i18n::t;
use amber_gpui::vault;
use gpui_kit::*;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::Spike;
use crate::day_card::{button, danger_button};
use crate::theme::Tokens;

/// 디스크에서 읽은 내용을 화면에 어떻게 반영할지
#[derive(Debug, PartialEq)]
pub enum DiskSync {
    /// 화면과 같다(우리 저장, touch, 다른 파일의 이벤트)
    None,
    /// 읽기 모드 — 본문만 바꾼다
    Reload,
    /// 편집 중이지만 고친 게 없다 — 초안까지 바꾼다
    ReloadDraft,
    /// 저장 안 된 편집이 있다 — 덮지 않고 묻는다
    Prompt,
}

/// dismissed = 이미 물어본(또는 '내 것 유지'로 넘긴) 디스크 내용 — 같은 변경으로 배너를 다시 띄우지 않는다
pub fn decide_disk_sync(
    disk: &str,
    body: &str,
    draft: Option<&str>,
    dismissed: Option<&str>,
) -> DiskSync {
    if disk == body {
        return DiskSync::None;
    }
    let Some(draft) = draft else {
        return DiskSync::Reload;
    };
    // 고친 게 없거나, 고친 결과가 마침 디스크와 같으면 잃을 것이 없다
    if draft == body || draft == disk {
        return DiskSync::ReloadDraft;
    }
    if Some(disk) == dismissed {
        return DiskSync::None;
    }
    DiskSync::Prompt
}

/// 트리나 열린 노트에 영향을 줄 수 있는 이벤트인가 — 읽기(접근) 이벤트와 우리 원자적 저장의 임시 파일만 닿은 것은 버린다
pub fn is_relevant(kind: &EventKind, paths: &[std::path::PathBuf]) -> bool {
    if matches!(kind, EventKind::Access(_)) {
        return false;
    }
    paths.is_empty()
        || paths
            .iter()
            .any(|p| !p.to_string_lossy().ends_with(".amber-tmp"))
}

/// 바깥이 고친 내용 — 물어보는 중인 것
pub struct DiskChange {
    pub path: String,
    pub disk: String,
    pub mtime: Option<i64>,
}

impl Spike {
    /// 노트 탭에 들어오거나 작업 폴더가 바뀌면 그 뿌리를 감시한다(이전 감시는 버린다)
    pub(crate) fn watch_notes_root(&mut self, cx: &mut Context<Self>) {
        let root = vault::notes(&self.db).abs("");
        let key = root.to_string_lossy().into_owned();
        if self.notes.watch_root.as_deref() == Some(key.as_str()) {
            return;
        }
        self.notes.watch_root = Some(key);
        self.notes.watcher = None;
        let (tx, rx) = mpsc::channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(e) = res
                && is_relevant(&e.kind, &e.paths)
            {
                let _ = tx.send(());
            }
        });
        let Ok(mut watcher) = watcher else {
            return;
        };
        // 감시를 못 걸면(권한, 사라진 폴더) 창 복귀 비교로만 동작한다
        if watcher
            .watch(Path::new(&root), RecursiveMode::Recursive)
            .is_err()
        {
            return;
        }
        self.notes.watcher = Some(watcher);
        let epoch = self.notes.watch_epoch.wrapping_add(1);
        self.notes.watch_epoch = epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let mut hit = false;
                loop {
                    match rx.try_recv() {
                        Ok(()) => hit = true,
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let alive = this
                    .update(cx, |s, cx| {
                        if s.notes.watch_epoch != epoch {
                            return false;
                        }
                        if hit {
                            s.reload_notes();
                            s.sync_from_disk(cx);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    return;
                }
            }
        })
        .detach();
    }

    /// 열린 노트를 디스크와 비교해 반영한다 — 파일 이벤트와 창 복귀가 부른다
    pub(crate) fn sync_from_disk(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.notes.selected.clone() else {
            return;
        };
        if self.notes.read_error.is_some() {
            return;
        }
        let tree = vault::notes(&self.db);
        // 지워졌거나 옮겨졌다 — 트리 갱신이 보여 준다. 열린 버퍼는 그대로 둔다
        let Ok(disk) = tree.read_file(&path) else {
            return;
        };
        let mtime = tree.mtime(&path);
        let draft = self.note_draft(cx);
        let asked = self
            .notes
            .disk_change
            .as_ref()
            .filter(|c| c.path == path)
            .map(|c| c.disk.clone())
            .or_else(|| self.notes.dismissed_disk.clone());
        match decide_disk_sync(&disk, &self.notes.body, draft.as_deref(), asked.as_deref()) {
            DiskSync::None => {
                // 내용은 같고 시각만 바뀌었다(우리 저장, touch) — 다음 ⌘S 가 충돌 창을 띄우지 않게 맞춘다
                if asked.is_none() && mtime.is_some() {
                    self.notes.mtime = mtime;
                }
            }
            DiskSync::Prompt => {
                self.notes.disk_change = Some(DiskChange { path, disk, mtime });
            }
            DiskSync::ReloadDraft => {
                let draft_open = self.notes.edit.is_some();
                self.open_note(&path, cx);
                if draft_open {
                    // 초안도 디스크 내용으로 — 편집은 이어 간다
                    self.restart_edit_with_body(cx);
                }
            }
            DiskSync::Reload => {
                // 읽던 자리를 지킨다
                let off = self.scroll.offset();
                self.open_note(&path, cx);
                self.scroll.set_offset(off);
            }
        }
    }

    /// 배너 — 저장 안 된 편집 중에 밖에서 파일이 바뀌었다. 정보라 무채색, 초안을 버리는 쪽은 빨강
    pub(crate) fn disk_banner(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let c = self.notes.disk_change.as_ref()?;
        if self.notes.selected.as_deref() != Some(c.path.as_str()) {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(14.))
                .mt(px(-6.))
                .mb(px(18.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .bg(k.surface_2)
                .text_size(px(13.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .child(t("notes.disk.changed")),
                        )
                        .child(t("notes.disk.detail")),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(
                            button("disk-keep", k, false, false)
                                .child(t("notes.disk.keep"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    // 디스크 시각을 받아들여 다음 ⌘S 가 같은 변경으로 다시 막히지 않게 한다
                                    if let Some(c) = this.notes.disk_change.take() {
                                        if c.mtime.is_some() {
                                            this.notes.mtime = c.mtime;
                                        }
                                        this.notes.dismissed_disk = Some(c.disk);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            danger_button("disk-load", k)
                                .child(crate::assets::icon("refresh", 13.).text_color(k.danger))
                                .child(t("notes.disk.load"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    // 초안을 버리고 디스크 내용으로 편집을 이어 간다
                                    if let Some(c) = this.notes.disk_change.take() {
                                        this.open_note(&c.path, cx);
                                        if this.notes.edit.is_some() {
                                            this.restart_edit_with_body(cx);
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    /// 초안을 지금 본문으로 바꾼다 — 입력칸 값은 창이 있어야 바뀌어서 다음 그림에서 넣는다
    pub(crate) fn restart_edit_with_body(&mut self, _cx: &mut Context<Self>) {
        if let Some(e) = self.notes.edit.as_mut() {
            e.dirty = false;
            self.notes.pending_draft = Some(self.notes.body.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiskSync, decide_disk_sync as d};

    #[test]
    fn like_note_watch() {
        assert_eq!(
            d("old", "old", None, None),
            DiskSync::None,
            "우리 저장, touch"
        );
        assert_eq!(d("old", "old", Some("typed"), None), DiskSync::None);
        assert_eq!(d("new", "old", None, None), DiskSync::Reload);
        assert_eq!(d("new", "old", Some("old"), None), DiskSync::ReloadDraft);
        assert_eq!(d("new", "old", Some("new"), None), DiskSync::ReloadDraft);
        assert_eq!(d("new", "old", Some("mine"), None), DiskSync::Prompt);
        assert_eq!(
            d("new", "old", Some("mine"), Some("new")),
            DiskSync::None,
            "같은 변경은 두 번 묻지 않는다"
        );
        assert_eq!(
            d("new", "old", Some("mine"), Some("older")),
            DiskSync::Prompt
        );
    }
}
