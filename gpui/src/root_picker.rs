// 작업 폴더 고르기 — 지금 앱 RootPicker.tsx, lib/workspace.ts. 노트 트리의 뿌리를 기본 보관함(앱 데이터 폴더)이나
// 고른 로컬 폴더로 바꾼다(IDE 의 '폴더 열기'). 최근 연 폴더 여섯 개를 기억한다(기본 보관함은 빼고).
// 저장 안 한 편집이 있으면 바꾸기 전에 묻는다 — 같은 트리의 다른 노트를 여는 것도 묻는데, 더 많이 잃는 이 전환이 무경고면 안 된다.

use amber_gpui::i18n::t;
use gpui_kit::*;

use crate::Spike;
use crate::menu::{MenuItem, select_menu};
use crate::note_edit::NotePrompt;
use crate::theme::Tokens;

const ROOT_KEY: &str = "amber.root.notes";
const RECENT_KEY: &str = "amber.recent-roots.notes";
const RECENT_MAX: usize = 6;

/// 지금 뿌리 — None 이면 기본 보관함
pub(crate) fn current_root(db: &amber_gpui::db::Db) -> Option<String> {
    amber_gpui::prefs::get(db, ROOT_KEY).filter(|r| !r.is_empty())
}

pub(crate) fn recent_roots(db: &amber_gpui::db::Db) -> Vec<String> {
    amber_gpui::prefs::get(db, RECENT_KEY)
        .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(&j).ok())
        .map(|a| {
            a.into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn last_segment(p: &str) -> String {
    p.split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or(p)
        .to_string()
}

enum Choice {
    Default,
    Recent(String),
    Open,
}

impl Spike {
    /// 뿌리 바꾸기 — 저장 안 한 편집이 있으면 먼저 묻는다. None = 기본 보관함
    pub(crate) fn request_root(&mut self, root: Option<String>) {
        if self.notes.edit.as_ref().is_some_and(|e| e.dirty) {
            self.notes.prompt = Some(NotePrompt::UnsavedRoot { root });
            return;
        }
        self.apply_root(root);
    }

    pub(crate) fn apply_root(&mut self, root: Option<String>) {
        amber_gpui::prefs::set(&self.db, ROOT_KEY, root.as_deref().unwrap_or(""));
        if let Some(r) = &root {
            let mut next = vec![r.clone()];
            next.extend(recent_roots(&self.db).into_iter().filter(|x| x != r));
            next.truncate(RECENT_MAX);
            amber_gpui::prefs::set(&self.db, RECENT_KEY, &serde_json::to_string(&next).unwrap());
        }
        let n = &mut self.notes;
        n.selected = None;
        n.edit = None;
        n.expanded.clear();
        n.active_dir.clear();
        n.blocks.clear();
        n.body.clear();
        self.reload_notes();
    }

    /// 고르기 판 — [기본 보관함(지금이 아니면)], 최근 폴더, 선, 폴더 열기…
    pub(crate) fn root_menu(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Deferred> {
        if !self.notes.root_menu {
            return None;
        }
        let cur = current_root(&self.db);
        let mut choices = Vec::new();
        let mut items = Vec::new();
        if cur.is_some() {
            choices.push(Choice::Default);
            items.push(MenuItem {
                label: t("settings.root.default"),
                sub: Some(t("settings.root.appData")),
                ..Default::default()
            });
        }
        for r in recent_roots(&self.db)
            .into_iter()
            .filter(|r| Some(r) != cur.as_ref())
        {
            items.push(MenuItem {
                label: last_segment(&r),
                sub: Some(r.clone()),
                ..Default::default()
            });
            choices.push(Choice::Recent(r));
        }
        items.push(MenuItem {
            label: t("settings.root.openFolder"),
            sub: Some(t("settings.root.openFolderDesc")),
            divider_before: !items.is_empty(),
            ..Default::default()
        });
        choices.push(Choice::Open);
        let (pick, close) = (cx.entity().downgrade(), cx.entity().downgrade());
        Some(select_menu(
            "root-menu",
            self.notes.root_anchor.get(),
            items,
            k,
            move |i, _, cx| {
                let choice = match choices.get(i) {
                    Some(Choice::Default) => Some(None),
                    Some(Choice::Recent(r)) => Some(Some(r.clone())),
                    Some(Choice::Open) | None => None,
                };
                let _ = pick.update(cx, |this, cx| {
                    this.notes.root_menu = false;
                    match choice {
                        Some(root) => this.request_root(root),
                        None => this.pick_root_folder(cx),
                    }
                    cx.notify();
                });
            },
            move |_, cx| {
                let _ = close.update(cx, |this, cx| {
                    this.notes.root_menu = false;
                    cx.notify();
                });
            },
        ))
    }

    /// macOS 폴더 고르기 창
    fn pick_root_folder(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t("settings.root.openDialogTitle").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                s.request_root(Some(dir.to_string_lossy().into_owned()));
                cx.notify();
            });
        })
        .detach();
    }
}
