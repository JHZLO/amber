// 모달 공통 — Esc 로 닫기 전에, 그 창을 연 뒤 사람이 입력칸에 친 게 있으면 한 번 묻는다(지금 앱 ui.tsx 의
// Modal typedRef → DiscardModal, common.closeDirty.*). X 와 취소는 묻지 않는다 — 누른 것 자체가 뜻이다.
// '친 것' = 입력칸의 Change(사람의 편집만 낸다, set_value 는 안 낸다). 모달마다 따로 센다(설정 위의 비밀번호 창은 따로).
// 확인 창에서 Esc 는 '계속 편집'이다.

use std::collections::{HashMap, HashSet};

use amber_gpui::i18n::t;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;

use crate::Spike;
use crate::day_card::{button, danger_button};
use crate::theme::Tokens;

/// 사람이 친 적 있는 입력칸
#[derive(Default)]
struct Typed(HashSet<EntityId>);

impl Global for Typed {}

/// gpui_kit::init 뒤에 — 새 입력칸마다 사람의 편집을 적는다
pub fn init(cx: &mut App) {
    cx.observe_new(|_: &mut InputState, _, cx: &mut Context<InputState>| {
        cx.subscribe_self(|_, ev: &InputEvent, cx| mark(ev, cx.entity_id(), cx))
            .detach();
    })
    .detach();
    cx.observe_new(
        |_: &mut TextareaState, _, cx: &mut Context<TextareaState>| {
            cx.subscribe_self(|_, ev: &InputEvent, cx| mark(ev, cx.entity_id(), cx))
                .detach();
        },
    )
    .detach();
}

fn mark(ev: &InputEvent, id: EntityId, cx: &mut App) {
    if matches!(ev, InputEvent::Change) {
        cx.default_global::<Typed>().0.insert(id);
    }
}

type Close = Box<dyn FnOnce(&mut Spike, &mut Window, &mut Context<Spike>)>;

#[derive(Default)]
pub struct CloseDirty {
    /// 열린 모달마다 — 연 뒤 친 게 있나(한 번 치면 닫힐 때까지), 그 창에서 본 입력칸
    open: HashMap<&'static str, (bool, HashSet<EntityId>)>,
    /// 묻는 중 — 버리면 부를 닫기, 계속하면 돌려줄 포커스
    ask: Option<(Close, Option<FocusHandle>)>,
}

impl Spike {
    /// 열린 모달과 그 입력칸 — Esc 갈래가 쓰는 이름으로
    fn modal_fields(&self) -> Vec<(&'static str, Vec<EntityId>)> {
        let mut out: Vec<(&'static str, Vec<EntityId>)> = Vec::new();
        if let Some(m) = self.notes.names.as_ref() {
            out.push(("names-notes", vec![m.input_id()]));
        }
        if let Some(m) = self.diagrams.names.as_ref() {
            out.push(("names-diagrams", vec![m.input_id()]));
        }
        if let Some(m) = self.notes.ai_modal.as_ref() {
            out.push(("note-ai", m.input.iter().map(|i| i.entity_id()).collect()));
        }
        if let Some(m) = self.notes.span_modal.as_ref() {
            out.push(("span-ai", m.field_ids()));
        }
        if let Some(m) = self.diagrams.ai.as_ref() {
            let mut ids = vec![m.ddl.entity_id(), m.instr.entity_id()];
            ids.extend(m.source.iter().map(|s| s.entity_id()));
            out.push(("dgm-ai", ids));
        }
        if let Some(a) = self.concepts.add.as_ref() {
            out.push((
                "concept-add",
                vec![
                    a.transcript.entity_id(),
                    a.instruction.entity_id(),
                    a.title.entity_id(),
                    a.summary.entity_id(),
                    a.tags.entity_id(),
                    a.body.entity_id(),
                ],
            ));
        }
        if let Some(a) = self.concepts.augment.as_ref() {
            out.push((
                "augment",
                vec![
                    a.instruction.entity_id(),
                    a.title.entity_id(),
                    a.summary.entity_id(),
                    a.tags.entity_id(),
                    a.body.entity_id(),
                ],
            ));
        }
        if self.settings.open {
            let mut ids = Vec::new();
            if let Some(r) = self.settings.rep.as_ref() {
                ids.extend(r.field_ids());
            }
            if let Some(a) = self.settings.ai.as_ref() {
                ids.extend([a.path.entity_id(), a.custom_input.entity_id()]);
            }
            if let Some(p) = self.settings.prompt_edit.as_ref() {
                ids.extend([p.name.entity_id(), p.text.entity_id()]);
            }
            out.push(("settings", ids));
        }
        if let Some(p) = self.settings.db_password.as_ref() {
            out.push(("db-password", vec![p.input.entity_id()]));
        }
        if let Some(c) = self.erd.conn.as_ref() {
            out.push(("erd-conn", c.field_ids()));
        }
        if let Some(s) = self.search.as_ref() {
            out.push(("search", vec![s.input.entity_id()]));
        }
        if let Some(a) = self.auth.as_ref() {
            out.push(("auth", vec![a.code.entity_id()]));
        }
        out
    }

    /// 그리기 전에 — 새로 뜬 모달은 처음부터 세고, 닫힌 모달은 잊는다. 처음 본 입력칸은 지난 기록을 지운다
    pub(crate) fn sync_close_dirty(&mut self, cx: &mut App) {
        let fields = self.modal_fields();
        let open = &mut self.close_dirty.open;
        open.retain(|k, _| fields.iter().any(|(f, _)| f == k));
        let typed = cx.default_global::<Typed>();
        for (key, ids) in fields {
            let (dirty, seen) = open.entry(key).or_default();
            for id in ids {
                if seen.insert(id) {
                    typed.0.remove(&id);
                } else if typed.0.contains(&id) {
                    *dirty = true;
                }
            }
        }
    }

    /// Esc 로 그 모달을 닫는다 — 연 뒤 친 게 있으면 먼저 묻고(버리면 close), 없으면 바로 close
    pub(crate) fn esc_close(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        close: impl FnOnce(&mut Spike, &mut Window, &mut Context<Spike>) + 'static,
    ) {
        self.sync_close_dirty(cx);
        if self.close_dirty.open.get(key).is_some_and(|(d, _)| *d) {
            let back = window.focused(cx);
            self.close_dirty.ask = Some((Box::new(close), back));
            // 확인 창이 키를 받게(뒤 입력칸에 글이 더 들어가지 않게)
            self.focus.focus(window, cx);
        } else {
            close(self, window, cx);
        }
        cx.notify();
    }

    /// 확인 창의 Esc, [계속 편집] — 묻기 전 자리로 돌아간다
    pub(crate) fn close_dirty_keep(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((_, back)) = self.close_dirty.ask.take() else {
            return false;
        };
        if let Some(h) = back {
            h.focus(window, cx);
        }
        cx.notify();
        true
    }

    /// 입력한 내용을 버릴까요? — 계속 편집 / 버리고 닫기(DiscardModal)
    pub(crate) fn close_dirty_modal(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        self.close_dirty.ask.as_ref()?;
        Some(crate::dialog::shell(
            k,
            440.,
            t("common.closeDirty.title"),
            div().child(t("common.closeDirty.body")).into_any_element(),
            vec![
                button("dirty-keep", k, false, false)
                    .child(t("common.unsaved.keep"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_dirty_keep(window, cx);
                    })),
                danger_button("dirty-discard", k)
                    .child(t("common.closeDirty.discard"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some((close, _)) = this.close_dirty.ask.take() {
                            close(this, window, cx);
                        }
                        cx.notify();
                    })),
            ],
            cx.listener(|this, _, window, cx| {
                this.close_dirty_keep(window, cx);
            }),
        ))
    }
}
