// 노트 인라인 질문(노션 댓글식) — 지금 앱 NoteComments.tsx. 읽기 화면에서 글을 드래그하면 [질문] 버튼이 뜨고,
// 오른쪽 패널에서 물으면 AI 가 짧게 답하고 사이드카(<이름>.comments.json)에 남는다. 물은 문장은 본문에
// 형광펜으로 칠한다(글을 고치지 않는다). 앵커 = 그려진 글자 문자열 + n번째 출현 — 지금 앱이 남긴 사이드카를 그대로 읽는다.
// 질문은 오른쪽 위 '질문 N' 으로 목록을 열어 본문 등장 순서로 본다. 한 줄을 누르면 스레드, 거기서 이어 묻는다.
// 코드 창, 알림 제목, 그림 안 글자는 세지 않는다(지금 앱은 textContent 전부). 아직: 형광펜을 눌러 스레드 열기(TextView 에 글자 히트 테스트가 없다 — 목록으로 연다), 기다리는 동안 훑는 빛(정지한 칠로 대신),
// 패널을 본문과 목차 사이 여백에 붙이기(오른쪽 위 고정), 답 고쳐 쓰기, 개념으로, 부분 수정.

use std::collections::HashMap;
use std::ops::Range;

use amber_core::ai::AskExchange;
use amber_gpui::comments;
use amber_gpui::i18n::{t, t_with};
use gpui_kit::base::text::{RangeHighlight, RenderedText};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{Value as Json, json};

use crate::Spike;
use crate::assets::icon;
use crate::day_card::{Confirm, Tag, button, primary_button};
use crate::notes_view::{text_states, time_ago};
use crate::theme::Tokens;

#[derive(Clone, PartialEq, Debug)]
pub enum Pop {
    Ask {
        anchor: String,
        occurrence: usize,
    },
    /// from_list = 목록에서 들어왔다(머리에 목록으로 돌아가는 버튼)
    View {
        id: String,
        from_list: bool,
    },
}

#[derive(Clone)]
pub struct Sel {
    pub anchor: String,
    pub occurrence: usize,
    /// 놓은 자리(창 좌표)와 그때의 본문 스크롤 — 굴리면 버튼이 글을 따라간다
    pub at: Point<Pixels>,
    pub scroll_y: Pixels,
    /// 선택이 걸친 블록의 본문 소스 구간 — [수정](부분 수정)이 받는다. 못 찾으면 단추가 없다
    pub block: Option<Range<usize>>,
}

/// 질문 패널을 붙일 자리를 재는 것들(창 좌표, 지난 그림) — 본문 칸, 목차(안 보이면 빈 자리), 읽기 줄
#[derive(Default, Clone)]
pub struct DockGeo {
    pub col: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub toc: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub wrap: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
}

#[derive(Default)]
pub struct Cmts {
    pub list: Vec<Json>,
    /// id → (글 덩어리 번호 — text_states 순서, 그 덩어리의 그려진 글자 범위, 본문 전체에서의 위치). 없으면 본문에서 사라진 앵커
    pub at: HashMap<String, (usize, Range<usize>, usize)>,
    pub sel: Option<Sel>,
    pub pop: Option<Pop>,
    pub list_open: bool,
    pub input: Option<Entity<TextareaState>>,
    /// 입력칸을 이 값으로 다시 만든다 — 입력칸은 창이 있어야 만들어진다(render 에서)
    pub rebuild: Option<String>,
    /// 답을 기다리는 자리(앵커, 출현, 되돌아갈 패널) — 패널을 닫아도 그 자리는 계속 칠한다
    pub asking: Option<(String, usize, Pop)>,
    pub error: Option<String>,
    /// 답을 기다리는 후속 질문 — 스레드 끝에 먼저 보인다
    pub pending_q: Option<(String, String)>,
    /// 고쳐 쓰는 답(턴 번호 — 0 = 첫 답)과 지시 입력칸(None = render 에서 만든다)
    pub revise: Option<(usize, Option<Entity<TextareaState>>)>,
    /// 마지막으로 마우스를 놓은 자리 — [질문] 버튼이 거기 뜬다
    pub mouse: Point<Pixels>,
    /// 덩어리마다 마지막으로 본 글자(파싱이 끝났는지 알려고)
    seen: Vec<RenderedText>,
}

type AskRx = futures::channel::oneshot::Receiver<
    Result<amber_core::ai::NoteAskResult, amber_core::ai::AiError>,
>;

fn s(v: &Json, k: &str) -> String {
    v.get(k)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn turns_of(c: &Json) -> Vec<(String, String, i64)> {
    let mut out = vec![(
        s(c, "question"),
        s(c, "answer"),
        c["createdAt"].as_i64().unwrap_or(0),
    )];
    for tu in c["followUps"].as_array().into_iter().flatten() {
        out.push((
            s(tu, "question"),
            s(tu, "answer"),
            tu["createdAt"].as_i64().unwrap_or(0),
        ));
    }
    out
}

/// noteAnchor.nthIndex 의 바이트판 — 출현은 한 글자씩 앞으로 세며 찾는다(겹친 출현도 센다)
fn nth_byte(hay: &str, anchor: &str, occ: usize) -> Option<usize> {
    if anchor.is_empty() {
        return None;
    }
    let mut from = 0;
    let mut found = None;
    for _ in 0..=occ {
        let i = from + hay.get(from..)?.find(anchor)?;
        found = Some(i);
        from = i + hay[i..].chars().next().map_or(1, char::len_utf8);
    }
    found
}

/// pos 앞에서 시작하는 출현 개수 — 선택한 자리가 몇 번째 출현인가
fn occurrence_at(hay: &str, anchor: &str, pos: usize) -> usize {
    let mut n = 0;
    while let Some(i) = nth_byte(hay, anchor, n) {
        if i >= pos {
            break;
        }
        n += 1;
    }
    n
}

impl Spike {
    /// 노트를 열 때 — 사이드카를 읽고, 글이 그려지는 대로 자리를 찾는다
    /// same = 같은 노트를 다시 읽었다(저장, 밖에서 고침) — 열어 둔 패널과 기다리는 답은 그대로 둔다
    pub(crate) fn load_note_comments(&mut self, rel: &str, same: bool, cx: &mut Context<Self>) {
        let list = comments::load_comments(&self.db, rel);
        let c = &mut self.notes.cmts;
        if same {
            c.list = list;
            c.at.clear();
            c.sel = None;
            c.seen.clear();
        } else {
            *c = Cmts {
                list,
                ..Default::default()
            };
        }
        for state in text_states(&self.notes.blocks) {
            cx.observe(&state, |this, _, cx| this.on_text_changed(cx))
                .detach();
        }
    }

    /// 덩어리가 알렸다 — 글이 새로 그려졌을 때만 자리를 다시 찾는다(칠하기도 알림을 내므로 그땐 넘긴다)
    fn on_text_changed(&mut self, cx: &mut Context<Self>) {
        let texts: Vec<RenderedText> = text_states(&self.notes.blocks)
            .iter()
            .map(|s| s.read(cx).rendered_text())
            .collect();
        if texts == self.notes.cmts.seen {
            return;
        }
        self.notes.cmts.seen = texts;
        self.locate_comments(cx);
        self.repaint_marks(cx);
        cx.notify();
    }

    /// 본문 전체 글자 = 글 덩어리(토글 안까지)를 이은 것, 그리고 덩어리마다 시작 위치.
    /// 지금 앱은 본문 상자의 textContent 에서 센다 — 코드 창, 알림 제목, 그림 글자는 여기 없어서
    /// 그 안에도 같은 문장이 있으면 n번째가 다른 자리를 가리킬 수 있다(알려진 차이)
    fn doc_text(&self, cx: &App) -> (String, Vec<usize>) {
        let mut doc = String::new();
        let mut starts = Vec::new();
        for state in text_states(&self.notes.blocks) {
            starts.push(doc.len());
            doc.push_str(state.read(cx).rendered_text().as_str());
        }
        (doc, starts)
    }

    /// 앵커의 자리 — n번째 출현이 사라졌으면 첫 출현으로. 덩어리를 넘는 앵커는 첫 덩어리 안만
    fn find_anchor(
        &self,
        doc: &(String, Vec<usize>),
        anchor: &str,
        occ: usize,
        cx: &App,
    ) -> Option<(usize, Range<usize>, usize)> {
        let (text, starts) = doc;
        let pos = nth_byte(text, anchor, occ).or_else(|| nth_byte(text, anchor, 0))?;
        let bi = starts.iter().rposition(|st| *st <= pos)?;
        let start = starts[bi];
        let len = text_states(&self.notes.blocks)
            .get(bi)?
            .read(cx)
            .rendered_text()
            .as_str()
            .len();
        let end = (pos - start + anchor.len()).min(len);
        Some((bi, (pos - start)..end, pos))
    }

    fn locate_comments(&mut self, cx: &mut Context<Self>) {
        let doc = self.doc_text(cx);
        let at = self
            .notes
            .cmts
            .list
            .iter()
            .filter_map(|c| {
                let occ = c["occurrence"].as_u64().unwrap_or(0) as usize;
                let hit = self.find_anchor(&doc, &s(c, "anchor"), occ, cx)?;
                Some((s(c, "id"), hit))
            })
            .collect();
        self.notes.cmts.at = at;
    }

    /// 누르면 떠 있던 [질문] 버튼을 거둔다
    pub(crate) fn note_mouse_down(&mut self) {
        self.notes.cmts.sel = None;
    }

    /// 놓은 자리 — 선택이 있으면 앵커(그려진 글자)와 출현 순번을 잡고 [질문] 버튼을 띄운다
    pub(crate) fn note_mouse_up(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.notes.cmts.mouse = at;
        let (doc, starts) = self.doc_text(cx);
        let origins = crate::notes_view::text_origins(&self.notes.body);
        let mut found = None;
        for (bi, (state, &start)) in text_states(&self.notes.blocks)
            .iter()
            .zip(&starts)
            .enumerate()
        {
            let st = state.read(cx);
            let Some(src) = st.selected_source_range().filter(|r| !r.is_empty()) else {
                continue;
            };
            let Some(r) = st.rendered_text().range_for_source(src.clone()) else {
                continue;
            };
            let a = start + r.start;
            let Some(anchor) = doc.get(a..start + r.end).map(str::to_string) else {
                continue;
            };
            // 한 글자짜리 선택에는 묻지 않는다(지금 앱과 같다)
            if anchor.trim().chars().count() < 2 {
                continue;
            }
            // 그 블록의 소스 구간 → 본문 줄(블록은 줄 머리에서 시작해 줄 끝에서 끝난다)
            let block = origins.get(bi).and_then(|lines| {
                let md = st.rendered_text().source().to_string();
                let b = crate::note::selection_block(&md, src.clone())?;
                let (ls, le) = (
                    md[..b.start].matches('\n').count(),
                    md[..b.end].matches('\n').count(),
                );
                Some(lines.get(ls)?.start..lines.get(le)?.end)
            });
            found = Some(Sel {
                occurrence: occurrence_at(&doc, &anchor, a),
                anchor,
                at,
                scroll_y: self.scroll.offset().y,
                block,
            });
            break;
        }
        self.notes.cmts.sel = found;
    }

    /// 형광펜(질문) + 찾기를 함께 칠한다 — 뒤에 칠한 것이 위로(지금 연 스레드, 그 위에 찾기)
    pub(crate) fn repaint_marks(&mut self, cx: &mut Context<Self>) {
        let k = Tokens::get(cx);
        let (q, q_cur): (Hsla, Hsla) = if k.dark {
            (rgb(0x523300).into(), rgb(0x784c00).into())
        } else {
            (rgb(0xfdefb4).into(), rgb(0xfce37e).into())
        };
        let c = &self.notes.cmts;
        let cur = match &c.pop {
            Some(Pop::View { id, .. }) => Some(id.clone()),
            _ => None,
        };
        let mut per: HashMap<usize, Vec<RangeHighlight>> = HashMap::new();
        let mut ordered: Vec<_> = c.at.iter().collect();
        ordered.sort_by_key(|(id, _)| Some(*id) == cur.as_ref());
        for (id, (bi, r, _)) in ordered {
            let color = if Some(id) == cur.as_ref() { q_cur } else { q };
            per.entry(*bi)
                .or_default()
                .push(RangeHighlight::new(r.clone(), color));
        }
        // 묻는 중이거나 질문 패널이 열린 자리 — 아직 저장 전이라 위 목록에 없다
        let pending = match (&c.asking, &c.pop) {
            (Some((a, o, _)), _) => Some((a.clone(), *o)),
            (None, Some(Pop::Ask { anchor, occurrence })) => Some((anchor.clone(), *occurrence)),
            _ => None,
        };
        if let Some((a, o)) = pending {
            let doc = self.doc_text(cx);
            if let Some((bi, r, _)) = self.find_anchor(&doc, &a, o, cx) {
                per.entry(bi)
                    .or_default()
                    .push(RangeHighlight::new(r, q_cur));
            }
        }
        for (bi, r, color) in self.page_find_marks(cx) {
            per.entry(bi)
                .or_default()
                .push(RangeHighlight::new(r, color));
        }
        for (bi, state) in text_states(&self.notes.blocks).iter().enumerate() {
            let marks = per.remove(&bi).unwrap_or_default();
            state.update(cx, |s, cx| {
                let _ = s.set_range_highlights(marks, cx);
            });
        }
    }

    pub(crate) fn open_pop(&mut self, pop: Pop, cx: &mut Context<Self>) {
        let c = &mut self.notes.cmts;
        c.error = None;
        c.sel = None;
        c.list_open = false;
        c.pop = Some(pop);
        c.revise = None;
        c.rebuild = Some(String::new());
        for state in text_states(&self.notes.blocks) {
            state.update(cx, |s, cx| s.clear_selection(cx));
        }
        self.repaint_marks(cx);
    }

    pub(crate) fn close_pop(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.notes.cmts;
        c.pop = None;
        c.input = None;
        c.revise = None;
        c.error = None;
        self.repaint_marks(cx);
    }

    /// render 에서 — 질문/후속 입력칸을 지금 패널에 맞게 새로 만든다
    pub(crate) fn apply_cmt_rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((turn, None)) = self.notes.cmts.revise {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, 4)
                    .submit_on_enter(true)
                    .placeholder(t("notes.cmt.revisePh"))
            });
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = ev {
                    this.submit_revise(cx);
                    cx.notify();
                }
            })
            .detach();
            input.update(cx, |s, cx| s.focus(window, cx));
            self.notes.cmts.revise = Some((turn, Some(input)));
        }
        let Some(value) = self.notes.cmts.rebuild.take() else {
            return;
        };
        let ask = matches!(self.notes.cmts.pop, Some(Pop::Ask { .. }));
        let input = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(if ask { 2 } else { 1 }, 6)
                .submit_on_enter(true)
                .placeholder(t(if ask {
                    "notes.cmt.askPh"
                } else {
                    "notes.cmt.followUpPh"
                }));
            s.set_value(value, window, cx);
            s
        });
        cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = ev {
                this.submit_question(cx);
                cx.notify();
            }
        })
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.notes.cmts.input = Some(input);
    }

    fn input_value(&self, cx: &App) -> String {
        self.notes
            .cmts
            .input
            .as_ref()
            .map(|i| i.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    /// AI 에 묻는다(tokio 위). 연결이 없으면 None — 보내기 단추가 이미 꺼져 있다
    fn ask_ai(
        &self,
        anchor: &str,
        question: &str,
        history: Option<Vec<AskExchange>>,
        revise: Option<String>,
    ) -> Option<AskRx> {
        let cfg = amber_gpui::ai_config::load_config(&self.db)
            .ok()
            .filter(|c| c.provider.is_some())?;
        Some(crate::ai::spawn(amber_core::ai::ai_note_ask(
            anchor.to_string(),
            question.to_string(),
            self.notes.body.clone(),
            history,
            revise,
            Some(cfg.model.clone()).filter(|m| !m.is_empty()),
            Some(cfg.cli_path.clone()).filter(|p| !p.is_empty()),
            cfg.provider.clone(),
            None,
            Some(amber_gpui::ai_config::output_lang(&cfg)),
        )))
    }

    /// 답 고쳐 쓰기 — 새 문답으로 덧붙이지 않고 그 답을 제자리에서 바꾼다("더 짧게", "한국어로" 처럼)
    pub(crate) fn submit_revise(&mut self, cx: &mut Context<Self>) {
        let c = &self.notes.cmts;
        let Some((turn, Some(input))) = c.revise.clone() else {
            return;
        };
        let instruction = input.read(cx).value().trim().to_string();
        if c.asking.is_some() || instruction.chars().count() < 2 {
            return;
        }
        let (Some(rel), Some(pop)) = (self.notes.selected.clone(), c.pop.clone()) else {
            return;
        };
        let Pop::View { id, .. } = &pop else {
            return;
        };
        let id = id.clone();
        let Some(cm) = c.list.iter().find(|x| s(x, "id") == id) else {
            return;
        };
        let Some((_, prev, _)) = turns_of(cm).into_iter().nth(turn) else {
            return;
        };
        let (anchor, occ) = (
            s(cm, "anchor"),
            cm["occurrence"].as_u64().unwrap_or(0) as usize,
        );
        let Some(rx) = self.ask_ai(&anchor, &instruction, None, Some(prev)) else {
            return;
        };
        let c = &mut self.notes.cmts;
        c.asking = Some((anchor, occ, pop));
        c.error = None;
        self.repaint_marks(cx);
        cx.spawn(async move |this, cx| {
            let r = crate::ai::wait(rx, cx.background_executor().clone()).await;
            let _ = this.update(cx, |s, cx| {
                s.notes.cmts.asking = None;
                match r {
                    Some(Ok(res)) => s.replace_answer(&rel, &id, turn, res),
                    Some(Err(e)) => s.notes.cmts.error = Some(crate::ai::err_text(&e)),
                    None => {}
                }
                s.repaint_marks(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// 디스크 기준으로 그 답만 갈아끼운다(요청 중 다른 저장이 있었어도)
    fn replace_answer(
        &mut self,
        rel: &str,
        id: &str,
        turn: usize,
        res: amber_core::ai::NoteAskResult,
    ) {
        let r = comments::mutate_comments(&self.db, rel, |mut list| {
            if let Some(c) = list.iter_mut().find(|c| s(c, "id") == id) {
                let slot = if turn == 0 {
                    Some(c)
                } else {
                    c.get_mut("followUps")
                        .and_then(Json::as_array_mut)
                        .and_then(|a| a.get_mut(turn - 1))
                };
                if let Some(t) = slot {
                    t["answer"] = json!(res.answer);
                    t["model"] = json!(res.meta.model);
                }
            }
            list
        });
        match r {
            Err(e) => self.notes.cmts.error = Some(t(&e)),
            Ok(next) if self.notes.selected.as_deref() == Some(rel) => {
                self.notes.cmts.list = next;
                self.notes.cmts.revise = None;
            }
            Ok(_) => {}
        }
    }

    /// 질문(새 스레드) 또는 이어 묻기 — 이어 물을 땐 그 스레드의 문답 전부를 문맥으로 싣는다
    pub(crate) fn submit_question(&mut self, cx: &mut Context<Self>) {
        let q = self.input_value(cx);
        let c = &self.notes.cmts;
        if c.asking.is_some() || q.chars().count() < 2 {
            return;
        }
        let Some(rel) = self.notes.selected.clone() else {
            return;
        };
        let Some(pop) = c.pop.clone() else {
            return;
        };
        let (anchor, occurrence, target, history) = match &pop {
            Pop::Ask { anchor, occurrence } => (anchor.clone(), *occurrence, None, None),
            Pop::View { id, .. } => {
                let Some(cm) = c.list.iter().find(|x| s(x, "id") == *id) else {
                    return;
                };
                let history = turns_of(cm)
                    .into_iter()
                    .map(|(question, answer, _)| AskExchange { question, answer })
                    .collect();
                let occ = cm["occurrence"].as_u64().unwrap_or(0) as usize;
                (s(cm, "anchor"), occ, Some(id.clone()), Some(history))
            }
        };
        let Some(rx) = self.ask_ai(&anchor, &q, history, None) else {
            return;
        };
        let c = &mut self.notes.cmts;
        c.asking = Some((anchor.clone(), occurrence, pop));
        c.error = None;
        if let Some(id) = &target {
            // 보낸 질문은 말풍선으로 먼저 올리고 입력칸을 비운다(실패하면 되돌린다)
            c.pending_q = Some((id.clone(), q.clone()));
            c.rebuild = Some(String::new());
        }
        self.repaint_marks(cx);
        cx.spawn(async move |this, cx| {
            let r = crate::ai::wait(rx, cx.background_executor().clone()).await;
            let _ = this.update(cx, |s, cx| {
                s.notes.cmts.asking = None;
                s.notes.cmts.pending_q = None;
                match r {
                    Some(Ok(res)) => s.save_answer(&rel, anchor, occurrence, target, q, res, cx),
                    Some(Err(e)) => {
                        s.notes.cmts.error = Some(crate::ai::err_text(&e));
                        if target.is_some() && s.notes.selected.as_deref() == Some(rel.as_str()) {
                            s.notes.cmts.rebuild = Some(q);
                        }
                    }
                    None => {}
                }
                s.repaint_marks(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// 답을 사이드카에 — 늘 디스크를 정본으로 읽어 고친다(요청 중 노트를 옮겨도 원래 노트에)
    #[allow(clippy::too_many_arguments)]
    fn save_answer(
        &mut self,
        rel: &str,
        anchor: String,
        occurrence: usize,
        target: Option<String>,
        question: String,
        res: amber_core::ai::NoteAskResult,
        cx: &mut Context<Self>,
    ) {
        let now = chrono::Local::now().timestamp_millis();
        let new_id = comments::new_comment_id();
        let turn = json!({
            "question": question, "answer": res.answer, "createdAt": now, "model": res.meta.model,
        });
        let r = comments::mutate_comments(&self.db, rel, |mut list| {
            match &target {
                None => {
                    let mut cm =
                        json!({ "id": new_id, "anchor": anchor, "occurrence": occurrence });
                    for (k, v) in turn.as_object().into_iter().flatten() {
                        cm[k] = v.clone();
                    }
                    list.push(cm);
                }
                Some(id) => {
                    if let Some(c) = list.iter_mut().find(|c| s(c, "id") == *id) {
                        match c.get_mut("followUps").and_then(Json::as_array_mut) {
                            Some(a) => a.push(turn.clone()),
                            None => c["followUps"] = json!([turn.clone()]),
                        }
                    }
                }
            }
            list
        });
        match r {
            Err(e) => self.notes.cmts.error = Some(t(&e)),
            Ok(next) => {
                if self.notes.selected.as_deref() != Some(rel) {
                    return;
                }
                let c = &mut self.notes.cmts;
                c.list = next;
                if target.is_none()
                    && matches!(&c.pop, Some(Pop::Ask { anchor: a, .. }) if *a == anchor)
                {
                    c.pop = Some(Pop::View {
                        id: new_id,
                        from_list: false,
                    });
                    c.rebuild = Some(String::new());
                }
                self.locate_comments(cx);
            }
        }
    }

    /// 스레드 지우기(확인 뒤) — 이어 물은 것까지 함께
    pub(crate) fn delete_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(rel) = self.notes.selected.clone() else {
            return;
        };
        let r = comments::mutate_comments(&self.db, &rel, |list| {
            list.into_iter().filter(|c| s(c, "id") != id).collect()
        });
        match r {
            Ok(next) => {
                let c = &mut self.notes.cmts;
                c.list = next;
                if matches!(&c.pop, Some(Pop::View { id: v, .. }) if v == id) {
                    c.pop = None;
                    c.input = None;
                }
                self.locate_comments(cx);
                self.repaint_marks(cx);
            }
            Err(e) => self.notes.cmts.error = Some(t(&e)),
        }
    }

    fn ask_delete(&mut self, cm: &Json) {
        self.confirm = Some(Confirm::Comment {
            id: s(cm, "id"),
            name: s(cm, "question"),
        });
    }

    /// 목록에서 연 스레드 — 본문의 그 문장으로 굴린다
    pub(crate) fn open_thread(&mut self, id: String, from_list: bool, cx: &mut Context<Self>) {
        if let Some((bi, r, _)) = self.notes.cmts.at.get(&id).cloned()
            && let Some(state) = text_states(&self.notes.blocks).get(bi)
        {
            state.update(cx, |s, cx| {
                let _ = s.reveal_range(r, cx);
            });
        }
        self.open_pop(Pop::View { id, from_list }, cx);
    }

    /// 선택 위의 [질문] 버튼(.cmt-fab-bar) — 창 좌표라 뿌리 판에 그린다
    pub(crate) fn comment_fab(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let c = &self.notes.cmts;
        let sel = c.sel.clone()?;
        if self.notes.edit.is_some() {
            return None;
        }
        // 답을 기다리는 동안엔 새 질문을 열지 않는다 — 개념으로 올리기는 된다
        let asking = c.asking.is_some();
        let fab = |id: &'static str| {
            div()
                .id(id)
                .tag(id)
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(11.))
                .py(px(5.))
                .rounded(px(6.))
                .border_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .shadow(k.shadow())
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.text)
                .cursor_pointer()
                .hover(|st| st.bg(k.surface_2))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let promote = {
            let anchor = sel.anchor.clone();
            fab("cmt-promote")
                .tooltip(crate::tip::tip(t("notes.cmt.promoteTip")))
                .child(icon("layers", 13.).text_color(k.text_3))
                .child(t("notes.cmt.promote"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    let Some(rel) = this.notes.selected.clone() else {
                        return;
                    };
                    this.notes.cmts.sel = None;
                    let target = crate::concept_add::PromoteTarget {
                        note_rel: rel,
                        selection: anchor.clone(),
                        note_body: this.notes.body.clone(),
                    };
                    this.open_promote(target, window, cx);
                    cx.notify();
                }))
        };
        Some(
            div()
                .absolute()
                // 지금 앱: 선택 끝 +6, 첫 줄 위 38. 우리는 놓은 자리만 안다 — 줄 가운데에서 한 줄 반 위로.
                // 본문을 굴리면 따라간다
                .left(sel.at.x + px(6.))
                .top((sel.at.y + (self.scroll.offset().y - sel.scroll_y) - px(48.)).max(px(8.)))
                .flex()
                .gap(px(4.))
                .when(!asking, |d| {
                    d.child(
                        div()
                            .id("cmt-fab")
                            .tag("cmt-fab")
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px(px(11.))
                            .py(px(5.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(k.border_strong)
                            .bg(k.surface)
                            .shadow(k.shadow())
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text)
                            .cursor_pointer()
                            .hover(|st| st.bg(k.surface_2))
                            .child(icon("message", 13.).text_color(k.text_3))
                            .child(t("notes.cmt.ask"))
                            // 누르는 순간 본문 선택이 풀리지 않게 — 앵커는 이미 잡아 두었다
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_pop(
                                    Pop::Ask {
                                        anchor: sel.anchor.clone(),
                                        occurrence: sel.occurrence,
                                    },
                                    cx,
                                );
                                cx.notify();
                            })),
                    )
                })
                .child(promote)
                // 소스 좌표를 찾은 선택만 고칠 수 있다(제목 #### 아래, 알림 본문 같은 곳은 없다)
                .children(sel.block.clone().map(|span| {
                    fab("cmt-edit")
                        .tooltip(crate::tip::tip(t("notes.cmt.editTip")))
                        .child(icon("scissors", 13.).text_color(k.text_3))
                        .child(t("notes.cmt.edit"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.notes.cmts.sel = None;
                            this.open_span_modal(
                                crate::note_span_ai::SpanMode::Selection,
                                Some(span.clone()),
                                cx,
                            );
                            cx.notify();
                        }))
                })),
        )
    }

    /// 본문 판 위의 '질문 N'(.cmt-list-trigger)과 목록/질문/스레드 패널(.cmt-pop)
    /// 패널 자리(지금 앱 NoteComments 의 dock) — 가로는 본문 오른쪽 끝 ~ 목차 왼쪽 끝 사이 여백(24 띄우고 260~380),
    /// 세로는 본문 줄 안에 갇힌다: 위는 스크롤 판 위 + 44(찾기 줄 자리), 아래는 본문 끝(또는 판 아래 - 12), 최소 140.
    /// 여백이 좁으면 None — 오른쪽 위 폴백. 판(카드) 안 좌표로 (왼쪽, 폭, 위, 최대 높이)
    fn cmt_dock(&self) -> Option<(Pixels, Pixels, Pixels, Pixels)> {
        const GAP: f32 = 24.;
        const MIN: f32 = 260.;
        const MAX: f32 = 380.;
        const PAD_TOP: f32 = 44.;
        const PAD_BOTTOM: f32 = 12.;
        const MIN_H: f32 = 140.;
        let g = &self.notes.dock_geo;
        let (col, toc, wrap) = (g.col.get(), g.toc.get(), g.wrap.get());
        let s = self.scroll.bounds();
        if col.size.width <= px(0.) || s.size.height <= px(0.) {
            return None;
        }
        let left = col.right() + px(GAP);
        let right = if toc.size.width > px(0.) {
            toc.left() - px(GAP)
        } else {
            px(self.view_w - 16.)
        };
        let avail = right - left;
        if avail < px(MIN) {
            return None;
        }
        let top_limit = s.top() + px(PAD_TOP);
        let bottom_limit = s.bottom() - px(PAD_BOTTOM);
        // 읽기 줄의 지금 위(굴린 만큼) — 지난 그림의 자리 대신 스크롤 값으로(목차와 같은 셈)
        let wrap_top = s.top()
            + px(crate::notes_view::DETAIL_PAD_T)
            + self.notes.head_h.get()
            + self.scroll.offset().y;
        let floor = (wrap_top + wrap.size.height).min(bottom_limit);
        let top = wrap_top
            .max(top_limit)
            .min(top_limit.max(floor - px(MIN_H)));
        let max_h = px(MIN_H).max(floor - top);
        Some((left - s.left(), avail.min(px(MAX)), top - s.top(), max_h))
    }

    pub(crate) fn comment_layer(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = &self.notes.cmts;
        if self.notes.edit.is_some() || self.notes.selected.is_none() {
            return None;
        }
        let dock = self.cmt_dock();
        if c.pop.is_none() && !c.list_open {
            if c.list.is_empty() {
                return None;
            }
            return Some(
                div()
                    .absolute()
                    .map(|d| match dock {
                        Some((left, _, top, _)) => d.left(left).top(top),
                        None => d.top(px(16.)).right(px(16.)),
                    })
                    .child(
                        div()
                            .id("cmt-trigger")
                            .tag("cmt-trigger")
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(11.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(k.border)
                            .bg(k.surface)
                            .shadow(vec![BoxShadow {
                                color: k.shadow_sm,
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(2.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text_2)
                            .cursor_pointer()
                            .hover(|st| {
                                st.bg(k.surface_2)
                                    .text_color(k.text)
                                    .border_color(k.border_strong)
                            })
                            .child(icon("message", 13.).text_color(k.text_3))
                            .child(t_with(
                                "notes.qlist.btn",
                                &[("n", &c.list.len().to_string())],
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notes.cmts.list_open = true;
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        let body = match c.pop.clone() {
            None => self.cmt_list(k, cx),
            Some(Pop::Ask { anchor, .. }) => self.cmt_ask(&anchor, k, cx),
            Some(Pop::View { id, from_list }) => self.cmt_thread(&id, from_list, k, cx)?,
        };
        Some(
            div()
                .absolute()
                .map(|d| match dock {
                    // 붙일 자리 — 높이는 상자일 뿐 패널은 내용만큼(넘치면 그 안에서 굴린다)
                    Some((left, w, top, max_h)) => d.left(left).top(top).w(w).h(max_h),
                    None => d.top(px(16.)).right(px(16.)).bottom(px(16.)).w(px(380.)),
                })
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("cmt-pop")
                        .tag("cmt-pop")
                        .flex()
                        .flex_col()
                        .max_h_full()
                        .p(px(12.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(k.border_strong)
                        .bg(k.surface)
                        .shadow(k.shadow())
                        .occlude()
                        .child(body),
                )
                .into_any_element(),
        )
    }

    fn icon_btn(
        id: impl Into<SharedString>,
        name: &str,
        k: &Tokens,
        danger: bool,
    ) -> Stateful<Div> {
        let id: SharedString = id.into();
        let (bg, fg) = if danger {
            (k.danger_soft, k.danger)
        } else {
            (k.surface_2, k.text_2)
        };
        div()
            .id(id.clone())
            .tag(id.clone())
            .group(id.clone())
            .flex()
            .flex_shrink_0()
            .p(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(move |st| st.bg(bg))
            .child(
                icon(name, if danger { 13. } else { 14. })
                    .text_color(k.text_3)
                    .group_hover(id, move |st| st.text_color(fg)),
            )
    }

    fn anchor_quote(a: &str, k: &Tokens, lines: usize) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .pl(px(8.))
            .border_l(px(2.))
            .border_color(k.border_strong)
            .text_size(px(11.5))
            .text_color(k.text_3)
            .line_clamp(lines)
            .child(format!("\u{201c}{a}\u{201d}"))
    }

    fn cmt_list(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let c = &self.notes.cmts;
        // 본문 등장 순서 — 앵커가 사라진 것은 끝에, 그 안에서는 단 순서
        let mut rows: Vec<&Json> = c.list.iter().collect();
        rows.sort_by_key(|x| {
            (
                c.at.get(&s(x, "id")).map_or(usize::MAX, |a| a.2),
                x["createdAt"].as_i64().unwrap_or(0),
            )
        });
        let now = chrono::Local::now().timestamp_millis();
        let glass = |pair: (Hsla, Hsla)| {
            linear_gradient(
                150.,
                linear_color_stop(pair.0, 0.),
                linear_color_stop(pair.1, 1.),
            )
        };
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(6.))
                    .mb(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(k.text)
                            .child(icon("message", 13.).text_color(k.text_3))
                            .child(t_with(
                                "notes.qlist.title",
                                &[("n", &c.list.len().to_string())],
                            )),
                    )
                    .child(
                        Self::icon_btn("cmt-list-close", "x", k, false).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.notes.cmts.list_open = false;
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(
                div()
                    .id("cmt-list-rows")
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows.into_iter().map(|cm| {
                        let id = s(cm, "id");
                        let turns = turns_of(cm);
                        let last = turns.last().map_or(0, |x| x.2);
                        let gone = !c.at.contains_key(&id);
                        let (open_id, del) = (id.clone(), cm.clone());
                        let row = SharedString::from(format!("cmt-row-{id}"));
                        div()
                            .id(row.clone())
                            .group(row.clone())
                            .flex()
                            .items_start()
                            .gap(px(4.))
                            .pt(px(3.))
                            .pb(px(3.))
                            .pl(px(3.))
                            .pr(px(5.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(k.glass_edge)
                            .bg(glass(k.glass_fill))
                            .shadow(k.glass_shadow(false))
                            .when(gone, |d| d.opacity(0.62))
                            .when(!gone, |d| {
                                let (fill, edge) = (glass(k.glass_fill_hover), k.glass_edge_hover);
                                d.hover(move |st| st.bg(fill).border_color(edge))
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!("cmt-open-{id}")))
                                    .tag(format!("cmt-open-{id}"))
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.))
                                    .pt(px(7.))
                                    .pb(px(7.))
                                    .pl(px(7.))
                                    .pr(px(5.))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(k.text)
                                            .line_clamp(2)
                                            .child(s(cm, "question")),
                                    )
                                    .child(Self::anchor_quote(&s(cm, "anchor"), k, 1))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .text_size(px(11.))
                                            .text_color(k.text_3)
                                            .child(time_ago(last, now))
                                            .when(gone, |d| {
                                                d.child(
                                                    div()
                                                        .px(px(4.))
                                                        .rounded(px(4.))
                                                        .border_1()
                                                        .border_color(k.border_strong)
                                                        .text_size(px(10.))
                                                        .child(t("notes.qlist.missing")),
                                                )
                                            })
                                            // 이어 물은 게 있을 때만 — 모든 카드에 '1' 을 달면 아무것도 구분하지 못한다
                                            .when(turns.len() > 1, |d| {
                                                d.child(
                                                    div()
                                                        .ml_auto()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(3.))
                                                        .px(px(6.))
                                                        .py(px(1.))
                                                        .rounded_full()
                                                        .border_1()
                                                        .border_color(k.glass_edge_accent)
                                                        .bg(glass(k.glass_fill_accent))
                                                        .text_size(px(10.))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(k.text_2)
                                                        .child(
                                                            icon("message", 9.)
                                                                .text_color(k.text_2),
                                                        )
                                                        .child(turns.len().to_string()),
                                                )
                                            }),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_thread(open_id.clone(), true, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .mt(px(7.))
                                    .opacity(0.)
                                    .group_hover(row, |st| st.opacity(1.))
                                    .child(
                                        Self::icon_btn(format!("cmt-del-{id}"), "trash", k, true)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.ask_delete(&del);
                                                cx.notify();
                                            })),
                                    ),
                            )
                    })),
            )
    }

    fn cmt_ask(&self, anchor: &str, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let asking = self.notes.cmts.asking.is_some();
        let can = !asking && self.input_value(cx).chars().count() >= 2;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(6.))
                    .mb(px(8.))
                    .child(Self::anchor_quote(anchor, k, 2))
                    .child(
                        Self::icon_btn("cmt-close", "x", k, false).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.close_pop(cx);
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(self.cmt_input_box(k))
            .children(self.cmt_error(k))
            .child(if asking {
                Self::cmt_waiting(t("notes.cmt.thinking"), k)
            } else {
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.))
                    .mt(px(8.))
                    .child(
                        button("cmt-cancel", k, false, false)
                            .child(t("common.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_pop(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        primary_button("cmt-submit", k, !can)
                            .child(icon("sparkles", 13.).text_color(k.text))
                            .child(t("notes.cmt.askAi"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_question(cx);
                                cx.notify();
                            })),
                    )
            })
    }

    fn cmt_thread(
        &self,
        id: &str,
        from_list: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let c = &self.notes.cmts;
        let cm = c.list.iter().find(|x| s(x, "id") == id)?.clone();
        let turns = turns_of(&cm);
        let last = turns.last().map_or(0, |x| x.2);
        let asking = c.asking.is_some();
        let pending = c
            .pending_q
            .clone()
            .filter(|(pid, _)| pid == id)
            .map(|(_, q)| q);
        let on_link = self.on_link(cx);
        let ai_ok = amber_gpui::ai_config::load_config(&self.db)
            .ok()
            .is_some_and(|c| c.provider.is_some());
        let can = ai_ok && !asking && self.input_value(cx).chars().count() >= 2;
        let del = cm.clone();
        let now = chrono::Local::now().timestamp_millis();
        Some(
            div()
                .flex()
                .flex_col()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(6.))
                        .mb(px(8.))
                        .when(from_list, |d| {
                            d.child(
                                Self::icon_btn("cmt-back", "chevron-left", k, false).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.close_pop(cx);
                                        this.notes.cmts.list_open = true;
                                        cx.notify();
                                    }),
                                ),
                            )
                        })
                        .child(Self::anchor_quote(&s(&cm, "anchor"), k, 2))
                        .child(
                            Self::icon_btn("cmt-close", "x", k, false).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.close_pop(cx);
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .id("cmt-thread")
                        .tag("cmt-thread")
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(turns.into_iter().enumerate().map(|(i, (q, a, _))| {
                            let group = SharedString::from(format!("cmt-turn-{i}"));
                            let revising = c.revise.as_ref().filter(|(t, _)| *t == i);
                            div()
                                .id(group.clone())
                                .group(group.clone())
                                .flex()
                                .flex_col()
                                .gap(px(11.))
                                .child(Self::cmt_msg(true, div().child(q), k))
                                .child(Self::cmt_msg(
                                    false,
                                    div().child(crate::markdown::text(
                                        SharedString::from(format!("cmt-a-{id}-{i}")),
                                        a,
                                        on_link.clone(),
                                        k,
                                        None,
                                    )),
                                    k,
                                ))
                                .child(match revising {
                                    Some((_, Some(input))) => self.revise_line(input, ai_ok, k, cx),
                                    Some((_, None)) => div(),
                                    None => Self::revise_action(i, group, asking || !ai_ok, k, cx),
                                })
                                .when(asking && revising.is_some(), |d| {
                                    d.child(Self::cmt_waiting(t("notes.cmt.revising"), k))
                                })
                        }))
                        .children(pending.map(|q| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(11.))
                                .child(Self::cmt_msg(true, div().child(q), k))
                                .child(Self::cmt_msg(
                                    false,
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(k.text_3)
                                        .child(t("notes.cmt.thinking")),
                                    k,
                                ))
                        })),
                )
                .children(self.cmt_error(k))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(6.))
                        .mt(px(10.))
                        .child(div().flex_1().min_w_0().child(self.cmt_input_box(k)))
                        .child(
                            primary_button("cmt-send", k, !can)
                                .h(px(34.))
                                .w(px(34.))
                                .px_0()
                                .justify_center()
                                .flex_shrink_0()
                                .child(icon("sparkles", 15.).text_color(k.text))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.submit_question(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .mt(px(8.))
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(k.border)
                        .text_size(px(11.5))
                        .text_color(k.text_3)
                        .child(time_ago(last, now))
                        .child(div().flex_1())
                        .when(!asking, |d| {
                            d.child(Self::icon_btn("cmt-delete", "trash", k, true).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.ask_delete(&del);
                                    cx.notify();
                                }),
                            ))
                        }),
                ),
        )
    }

    /// 말풍선 — 내 말은 오른쪽 강조 유리, 답은 왼쪽 면 유리. 아바타에서 뻗어 나오고 꼬리는 위 모서리
    fn cmt_msg(me: bool, content: Div, k: &Tokens) -> Div {
        let (fill, edge) = if me {
            (k.glass_fill_accent, k.glass_edge_accent)
        } else {
            (k.glass_fill, k.glass_edge)
        };
        let bg = linear_gradient(
            150.,
            linear_color_stop(fill.0, 0.),
            linear_color_stop(fill.1, 1.),
        );
        let avatar = div()
            .flex_none()
            .size(px(24.))
            .mt(px(2.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(edge)
            .bg(bg.clone())
            .shadow(k.glass_shadow(false))
            .child(
                icon(if me { "user" } else { "sparkles" }, 12.).text_color(if me {
                    k.text_2
                } else {
                    k.text_3
                }),
            );
        let bubble = div()
            .min_w_0()
            // 두 아바타가 각자 차선을 갖는다 — 제 아바타(24+8)도, 건너편 아바타 자리(32)도 넘지 않는다
            .max_w(px(380. - 24. - 2. - 64.))
            .px(px(14.))
            .py(px(11.))
            .text_size(px(13.))
            .line_height(relative(1.68))
            .text_color(k.text)
            .border_1()
            .border_color(edge)
            .bg(bg)
            .shadow(k.glass_shadow(false))
            .when(me, |d| {
                d.rounded_tl(px(14.))
                    .rounded_tr(px(4.))
                    .rounded_b(px(14.))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!me, |d| {
                d.rounded_tl(px(4.)).rounded_tr(px(14.)).rounded_b(px(14.))
            })
            .child(content);
        div()
            .flex()
            .items_start()
            .gap(px(8.))
            .when(me, |d| d.flex_row_reverse())
            .child(avatar)
            .child(bubble)
    }

    fn cmt_input_box(&self, k: &Tokens) -> Div {
        match self.notes.cmts.input.as_ref() {
            Some(input) => Self::input_box(input, k),
            None => div(),
        }
    }

    /// 답마다 붙는 '고쳐 쓰기'(.cmt-turn-act) — 평소엔 숨어 있다가 그 문답에 손이 오면 드러난다. 답 쪽(왼쪽)에 선다
    fn revise_action(
        turn: usize,
        group: SharedString,
        disabled: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let (bg, fg) = (k.surface_2, k.text_2);
        let id = SharedString::from(format!("cmt-revise-{turn}"));
        div().flex().mt(px(-4.)).pl(px(36.)).min_h(px(18.)).child(
            div()
                .id(id.clone())
                .tag(id)
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(5.))
                .text_size(px(11.5))
                .text_color(k.text_3)
                .opacity(0.)
                .group_hover(group, |st| st.opacity(1.))
                .when(!disabled, |d| {
                    d.cursor_pointer()
                        .hover(move |st| st.bg(bg).text_color(fg))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.notes.cmts.revise = Some((turn, None));
                            this.notes.cmts.error = None;
                            cx.notify();
                        }))
                })
                .child(icon("pencil", 11.).text_color(k.text_3))
                .child(t("notes.cmt.revise")),
        )
    }

    /// 고쳐 쓰기 지시 줄(.cmt-revise) — 후속 질문 줄과 같은 치수. Esc 로 접는다
    fn revise_line(
        &self,
        input: &Entity<TextareaState>,
        ai_ok: bool,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let asking = self.notes.cmts.asking.is_some();
        let can = ai_ok && !asking && input.read(cx).value().trim().chars().count() >= 2;
        div()
            .flex()
            .items_end()
            .gap(px(6.))
            .mt(px(6.))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.notes.cmts.revise = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(div().flex_1().min_w_0().child(Self::input_box(input, k)))
            .child(
                primary_button("cmt-revise-send", k, !can)
                    .h(px(34.))
                    .w(px(34.))
                    .px_0()
                    .justify_center()
                    .flex_shrink_0()
                    .child(icon("sparkles", 15.).text_color(k.text))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.submit_revise(cx);
                        cx.notify();
                    })),
            )
    }

    fn input_box(input: &Entity<TextareaState>, k: &Tokens) -> Div {
        // 여러 줄 입력은 제 안쪽 여백(small = 위아래 2, 옆 8)을 가진다 — 바깥 상자는 나머지만(.textarea: 6 11, 34px)
        div()
            .rounded(px(6.))
            .border_1()
            .border_color(k.border_strong)
            .bg(k.surface)
            .px(px(3.))
            .py(px(4.))
            .child(
                Textarea::new(input)
                    .appearance(false)
                    .small()
                    .text_size(px(13.))
                    .line_height(px(20.)),
            )
    }

    fn cmt_waiting(label: String, k: &Tokens) -> Div {
        div()
            .pt(px(6.))
            .pb(px(4.))
            .text_size(px(12.5))
            .text_color(k.text_3)
            .child(label)
    }

    fn cmt_error(&self, k: &Tokens) -> Option<Div> {
        let e = self.notes.cmts.error.clone()?;
        Some(
            div()
                .mt(px(8.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(6.))
                .border_l(px(3.))
                .border_color(k.danger)
                .bg(k.danger_soft)
                .text_size(px(13.))
                .text_color(k.danger)
                .child(e),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{nth_byte, occurrence_at};

    #[test]
    fn anchors_count_overlapping_occurrences() {
        assert_eq!(nth_byte("aaa", "aa", 0), Some(0));
        assert_eq!(nth_byte("aaa", "aa", 1), Some(1), "한 글자씩 앞으로");
        assert_eq!(nth_byte("aaa", "aa", 2), None);
        assert_eq!(nth_byte("가나 가나", "가나", 1), Some(7));
        assert_eq!(occurrence_at("가나 가나 가나", "가나", 14), 2);
        assert_eq!(occurrence_at("가나 가나", "가나", 0), 0);
        assert_eq!(nth_byte("abc", "", 0), None);
    }
}
