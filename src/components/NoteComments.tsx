// 필기노트 인라인 질문(노션 댓글식) 레이어 — 읽기 모드 전용.
// 드래그 선택 → "질문" 플로팅 버튼 → 우측 고정 패널에서 질문 → AI 짧은 답변 → 사이드카에 저장.
// 저장된 질문은 본문 텍스트에 하이라이트(CSS Custom Highlight API, DOM 무변경)로 표시되고
// 클릭하면 우측 패널(항상 1개)로 스레드를 본다. 패널에서 후속 질문을 이어갈 수 있고
// 이전 문답이 AI 에 문맥으로 전달된다. 앵커 = "렌더된 텍스트 문자열 + n번째 출현".
//
// 문장을 클릭하는 길 외에 **글 단위 목록**(listOpen)이 하나 더 있다: 이 노트에 단 질문이
// 본문 등장 순서로 쫘르르 뜨고, 한 줄을 누르면 그 스레드 뷰로 넘어간다. 그래서 앵커 문장이
// 사라져도 질문은 목록으로 계속 닿는다 — 예전처럼 "연결이 끊겼다"고 알릴 이유가 없다.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { AppConfig } from "../lib/config";
import { aiNoteAsk, friendlyError } from "../lib/ai";
import {
  loadComments,
  newCommentId,
  saveComments,
  type AskTurn,
  type NoteComment,
} from "../lib/comments";
import { Markdown } from "./Markdown";
import { AiThinking, timeAgo, Tooltip } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

const HIGHLIGHT_KEY = "note-q";
const CURRENT_KEY = "note-q-cur";

// 패널은 우측 여백에 고정(CSS)이라 좌표를 들고 다니지 않는다 — 상태 하나 = 패널 최대 1개
type Pop =
  | { kind: "ask"; anchor: string; occurrence: number }
  // fromList = 목록에서 들어온 스레드 (헤드에 목록으로 돌아가는 버튼이 붙는다)
  | { kind: "view"; id: string; fromList?: boolean };

/** container 기준 텍스트 오프셋 (Range.toString 은 블록 개행을 추가하지 않아 textContent 와 동일 공간) */
function offsetIn(container: Node, node: Node, offset: number): number {
  const r = document.createRange();
  r.selectNodeContents(container);
  try {
    r.setEnd(node, offset);
  } catch {
    return 0;
  }
  return r.toString().length;
}

/** fullText 에서 anchor 의 occurrence 번째 출현 위치 (없으면 -1) */
function nthIndex(fullText: string, anchor: string, occurrence: number): number {
  if (!anchor) return -1;
  let idx = -1;
  for (let i = 0; i <= occurrence; i++) {
    idx = fullText.indexOf(anchor, idx + 1);
    if (idx === -1) return -1;
  }
  return idx;
}

/** container 의 텍스트 공간에서 [idx, idx+len) 구간을 Range 로 복원 (없으면 null).
 *  위치를 따로 받는 이유: 목록 정렬에 그 위치(본문 등장 순서)를 그대로 쓴다. */
function rangeAt(container: HTMLElement, idx: number, len: number): Range | null {
  const endIdx = idx + len;
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let acc = 0;
  let startNode: Text | null = null;
  let startOffset = 0;
  let endNode: Text | null = null;
  let endOffset = 0;
  let n: Node | null;
  while ((n = walker.nextNode())) {
    const t = n as Text;
    const len = t.data.length;
    if (!startNode && acc + len > idx) {
      startNode = t;
      startOffset = idx - acc;
    }
    if (startNode && acc + len >= endIdx) {
      endNode = t;
      endOffset = endIdx - acc;
      break;
    }
    acc += len;
  }
  if (!startNode || !endNode) return null;
  const r = document.createRange();
  r.setStart(startNode, startOffset);
  r.setEnd(endNode, endOffset);
  return r;
}

type HighlightRegistry = {
  set: (key: string, highlight: unknown) => void;
  delete: (key: string) => void;
};
function highlightRegistry(): HighlightRegistry | null {
  const css = CSS as unknown as { highlights?: HighlightRegistry };
  const HL = (window as unknown as { Highlight?: unknown }).Highlight;
  return css.highlights && HL ? css.highlights : null;
}

export function NoteCommentLayer({
  noteRel,
  body,
  containerRef,
  config,
  onCountChange,
  onPromote,
  listOpen = false,
  onListOpenChange,
}: {
  noteRel: string;
  body: string;
  containerRef: RefObject<HTMLDivElement | null>;
  config: AppConfig | null;
  onCountChange?: (n: number) => void;
  /** 선택 영역을 개념으로 승격 (NotesView 가 모달을 연다). 선택 텍스트를 넘긴다 */
  onPromote?: (selection: string) => void;
  /** 질문 목록 패널 표시 — 여는 버튼은 노트 툴바(NotesView)에 있어 상태를 위에서 들고 있다 */
  listOpen?: boolean;
  onListOpenChange?: (open: boolean) => void;
}) {
  const [comments, setComments] = useState<NoteComment[]>([]);
  // 선택 정보는 "선택하는 순간" 미리 계산해 둔다 — 버튼 클릭 시점의 라이브 셀렉션에
  // 의존하면 WebKit 이 mousedown 에서 선택을 해제하는 경우 조용히 실패한다.
  const [selInfo, setSelInfo] = useState<{
    x: number;
    y: number;
    bottom: number;
    anchor: string;
    occurrence: number;
  } | null>(null);
  const [pop, setPop] = useState<Pop | null>(null);
  const [question, setQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [askError, setAskError] = useState<string | null>(null);
  // 답변 대기 중인 후속 질문 — 해당 스레드 끝에 말풍선으로 먼저 보여준다
  const [pendingQ, setPendingQ] = useState<{ id: string; q: string } | null>(
    null,
  );
  // 답변 고쳐 쓰기 — turn 인덱스(0 = 첫 문답, 1.. = followUps)와 지시문 입력
  const [revising, setRevising] = useState<number | null>(null);
  const [reviseText, setReviseText] = useState("");
  // 패널을 놓을 자리: 본문 오른쪽 끝 ~ 목차 왼쪽 끝 사이의 여백(실측). 좁으면 null(우측 오버레이 폴백)
  const [dock, setDock] = useState<{ left: number; width: number } | null>(null);

  const popRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const threadRef = useRef<HTMLDivElement>(null);
  const followUpRef = useRef<HTMLTextAreaElement>(null);
  const rangesRef = useRef<{ id: string; range: Range }[]>([]);
  const relRef = useRef(noteRel);
  relRef.current = noteRel;

  // 본문에서의 앵커 위치. pos = 텍스트 오프셋(목록 정렬 = 본문 등장 순서),
  // missing = 앵커 문장이 수정돼 본문에서 못 찾은 스레드(목록에서 조용히 표시만 한다).
  const [anchors, setAnchors] = useState<{
    pos: Map<string, number>;
    missing: Set<string>;
  }>({ pos: new Map(), missing: new Set() });

  const notifyCount = useCallback(
    (list: NoteComment[]) => onCountChange?.(list.length),
    [onCountChange],
  );

  /** 스레드 열기. 목록에서 왔으면 본문의 그 문장으로 스크롤해 준다(클릭으로 왔으면 이미 보인다) */
  const openThread = useCallback(
    (id: string, fromList: boolean) => {
      setQuestion("");
      setAskError(null);
      setRevising(null);
      setReviseText("");
      setPop({ kind: "view", id, fromList });
      if (!fromList) return;
      onListOpenChange?.(false);
      const hit = rangesRef.current.find((r) => r.id === id);
      // Range 는 스크롤 대상이 못 되므로 앵커가 걸린 요소를 올린다 — 문장 단위면 충분하다
      hit?.range.startContainer.parentElement?.scrollIntoView({
        block: "center",
        behavior: "smooth",
      });
    },
    [onListOpenChange],
  );

  // 노트가 바뀌면 사이드카 로드
  useEffect(() => {
    let alive = true;
    setPop(null);
    setSelInfo(null);
    loadComments(noteRel).then((list) => {
      if (!alive) return;
      setComments(list);
      notifyCount(list);
    });
    return () => {
      alive = false;
    };
  }, [noteRel, notifyCount]);

  // 앵커 복원: 위치(목록 정렬용) → Range → CSS Custom Highlight.
  // 하이라이트를 못 쓰는 환경에서도 위치 계산과 스크롤은 그대로 살려 둔다(목록은 계속 동작).
  useEffect(() => {
    const c = containerRef.current;
    rangesRef.current = [];
    if (!c) return;
    const fullText = c.textContent ?? "";
    const found: { id: string; range: Range }[] = [];
    const pos = new Map<string, number>();
    const missing = new Set<string>();
    for (const cm of comments) {
      // 노트가 수정돼 n번째 출현이 사라졌으면 첫 출현으로 폴백
      let idx = nthIndex(fullText, cm.anchor, cm.occurrence);
      if (idx === -1) idx = nthIndex(fullText, cm.anchor, 0);
      if (idx === -1) {
        missing.add(cm.id);
        continue;
      }
      pos.set(cm.id, idx);
      const r = rangeAt(c, idx, cm.anchor.length);
      if (r) found.push({ id: cm.id, range: r });
      else missing.add(cm.id);
    }
    rangesRef.current = found;
    setAnchors({ pos, missing });
    const registry = highlightRegistry();
    if (!registry) return;
    const HL = (
      window as unknown as { Highlight: new (...r: Range[]) => unknown }
    ).Highlight;
    if (found.length) registry.set(HIGHLIGHT_KEY, new HL(...found.map((f) => f.range)));
    else registry.delete(HIGHLIGHT_KEY);
    return () => {
      try {
        registry.delete(HIGHLIGHT_KEY);
      } catch {
        /* noop */
      }
    };
  }, [comments, body, containerRef]);

  // 지금 열려 있는 스레드의 문장만 진하게. 목록에서 들어왔을 때 "본문의 어디였는지"가
  // 보여야 하기 때문이다 — 그 한 건만 덮어 칠하므로 note-q 위에 얹는다.
  useEffect(() => {
    const registry = highlightRegistry();
    if (!registry) return;
    const id = pop?.kind === "view" ? pop.id : null;
    const hit = id ? rangesRef.current.find((r) => r.id === id) : null;
    if (!hit) {
      registry.delete(CURRENT_KEY);
      return;
    }
    const HL = (
      window as unknown as { Highlight: new (...r: Range[]) => unknown }
    ).Highlight;
    registry.set(CURRENT_KEY, new HL(hit.range));
    return () => {
      try {
        registry.delete(CURRENT_KEY);
      } catch {
        /* noop */
      }
    };
  }, [pop, comments]);

  // 드래그 선택 감시 → 앵커/출현 인덱스/버튼 위치를 그 자리에서 계산 (rAF 스로틀)
  useEffect(() => {
    let raf = 0;
    const compute = () => {
      raf = 0;
      const c = containerRef.current;
      const sel = window.getSelection();
      if (!c || !sel || sel.rangeCount === 0 || sel.isCollapsed) {
        setSelInfo(null);
        return;
      }
      const r = sel.getRangeAt(0);
      if (!c.contains(r.startContainer) || !c.contains(r.endContainer)) {
        setSelInfo(null);
        return;
      }
      const startOff = offsetIn(c, r.startContainer, r.startOffset);
      const endOff = offsetIn(c, r.endContainer, r.endOffset);
      if (endOff <= startOff) {
        setSelInfo(null);
        return;
      }
      const fullText = c.textContent ?? "";
      const anchor = fullText.slice(startOff, endOff);
      if (anchor.trim().length < 2) {
        setSelInfo(null);
        return;
      }
      // 선택 지점이 몇 번째 출현인지 (앞쪽 출현 개수 = 0-based 인덱스)
      let occ = 0;
      let i = fullText.indexOf(anchor);
      while (i !== -1 && i < startOff) {
        occ++;
        i = fullText.indexOf(anchor, i + 1);
      }
      const rect = r.getBoundingClientRect();
      if (!rect || (rect.width === 0 && rect.height === 0)) {
        setSelInfo(null);
        return;
      }
      setSelInfo({
        x: rect.right,
        y: rect.top,
        bottom: rect.bottom,
        anchor,
        occurrence: occ,
      });
    };
    const onSel = () => {
      if (!raf) raf = requestAnimationFrame(compute);
    };
    document.addEventListener("selectionchange", onSel);
    // 스크롤해도 버튼이 선택 위치를 따라가게 재계산
    window.addEventListener("scroll", onSel, true);
    return () => {
      document.removeEventListener("selectionchange", onSel);
      window.removeEventListener("scroll", onSel, true);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [containerRef]);

  // 하이라이트 클릭 → 보기 팝오버 (Custom Highlight 는 이벤트가 없어 캐럿 히트테스트로)
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const c = containerRef.current;
      if (!c || !(e.target instanceof Node) || !c.contains(e.target)) return;
      const doc = document as Document & {
        caretRangeFromPoint?: (x: number, y: number) => Range | null;
      };
      const caret = doc.caretRangeFromPoint?.(e.clientX, e.clientY);
      if (!caret) return;
      for (const { id, range } of rangesRef.current) {
        try {
          if (range.isPointInRange(caret.startContainer, caret.startOffset)) {
            openThread(id, false);
            return;
          }
        } catch {
          /* 노드가 교체된 stale range — 무시 */
        }
      }
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [containerRef, openThread]);

  // 패널 닫기: 바깥 클릭 / Esc (답변 생성 중 Esc 는 무시).
  // 우측에 고정된 패널이라 스크롤로는 닫지 않는다 — 본문을 훑으며 스레드를 이어갈 수 있게.
  useEffect(() => {
    if (!pop) return;
    const down = (e: MouseEvent) => {
      if (popRef.current && !popRef.current.contains(e.target as Node))
        setPop(null);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !asking) setPop(null);
    };
    document.addEventListener("mousedown", down);
    window.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", down);
      window.removeEventListener("keydown", key);
    };
  }, [pop, asking]);

  // 목록 닫기: 바깥 클릭 / Esc. 여는 버튼(.cmt-list-trigger)은 예외다 —
  // mousedown 으로 먼저 닫으면 뒤이은 click 의 토글이 도로 열어 버려 눌러도 안 닫힌다.
  useEffect(() => {
    if (!listOpen || pop) return;
    const down = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest?.(".cmt-list-trigger")) return;
      if (listRef.current && !listRef.current.contains(e.target as Node))
        onListOpenChange?.(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onListOpenChange?.(false);
    };
    document.addEventListener("mousedown", down);
    window.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", down);
      window.removeEventListener("keydown", key);
    };
  }, [listOpen, pop, onListOpenChange]);

  // 패널 자리 실측: 본문(.markdown) 오른쪽 끝 ~ 목차(.note-toc) 왼쪽 끝 사이 여백에 끼운다.
  // 그 여백을 넘치지 않게 폭을 줄이고, 여백이 너무 좁으면(좁은 창·목차 숨김) 우측 오버레이로 폴백.
  useEffect(() => {
    if (!pop && !listOpen) return;
    const GAP = 24; // 본문/목차와 띄울 간격
    const MIN = 260; // 이보다 좁은 여백이면 폴백(우측 오버레이) — 너무 좁은 패널 방지
    const MAX = 380; // 패널 최대 폭
    const measure = () => {
      const content = containerRef.current;
      if (!content) return setDock(null);
      const cRect = content.getBoundingClientRect();
      const toc = content
        .closest(".note-read-wrap")
        ?.querySelector(".note-toc") as HTMLElement | null;
      const leftBound = cRect.right + GAP;
      const rightBound =
        toc && toc.offsetParent !== null
          ? toc.getBoundingClientRect().left - GAP
          : window.innerWidth - 16;
      const avail = rightBound - leftBound;
      if (avail < MIN) return setDock(null);
      setDock({ left: leftBound, width: Math.min(MAX, avail) });
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [pop, listOpen, containerRef]);

  // "질문" 버튼: 선택 시점에 계산해 둔 정보로 패널을 연다 (라이브 셀렉션에 의존하지 않음)
  function openAsk() {
    const si = selInfo;
    if (!si) return;
    setQuestion("");
    setAskError(null);
    setPop({ kind: "ask", anchor: si.anchor, occurrence: si.occurrence });
    onListOpenChange?.(false); // 새 질문을 쓰는 중엔 목록을 뒤에 남겨 두지 않는다
    setSelInfo(null);
    window.getSelection()?.removeAllRanges();
  }

  // "개념으로": 선택 텍스트를 위로 넘겨 승격 모달을 연다
  function openPromote() {
    const si = selInfo;
    if (!si || !onPromote) return;
    onPromote(si.anchor);
    setSelInfo(null);
    window.getSelection()?.removeAllRanges();
  }

  async function submitAsk() {
    if (!pop || pop.kind !== "ask" || !config || asking) return;
    const q = question.trim();
    if (q.length < 2) return;
    const rel = noteRel;
    const { anchor, occurrence } = pop;
    setAsking(true);
    setAskError(null);
    try {
      const { answer, meta } = await aiNoteAsk({
        selection: anchor,
        question: q,
        noteMarkdown: body,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
      });
      const cm: NoteComment = {
        id: newCommentId(),
        anchor,
        occurrence,
        question: q,
        answer,
        createdAt: Date.now(),
        model: meta.model,
      };
      // 항상 디스크 기준으로 병합-저장 (요청 중 노트를 이동했어도 원래 노트에 안전히 저장)
      const list = await loadComments(rel);
      const next = [...list, cm];
      await saveComments(rel, next);
      if (relRef.current === rel) {
        setComments(next);
        notifyCount(next);
        setQuestion("");
        setPop((p) => (p && p.kind === "ask" ? { kind: "view", id: cm.id } : p));
      }
    } catch (e) {
      setAskError(friendlyError(e));
    } finally {
      setAsking(false);
    }
  }

  // 후속 질문: 스레드의 이전 문답 전체를 문맥으로 실어 보내고, 답을 스레드 끝에 붙인다
  async function submitFollowUp() {
    if (!pop || pop.kind !== "view" || !config || asking) return;
    const target = comments.find((c) => c.id === pop.id);
    if (!target) return;
    const q = question.trim();
    if (q.length < 2) return;
    const rel = noteRel;
    const history = [
      { question: target.question, answer: target.answer },
      ...(target.followUps ?? []).map((t) => ({
        question: t.question,
        answer: t.answer,
      })),
    ];
    setAsking(true);
    setAskError(null);
    setPendingQ({ id: target.id, q });
    setQuestion("");
    // 방금 보낸 질문(pending 말풍선)이 보이게 스레드 맨 아래로
    requestAnimationFrame(() => {
      const t = threadRef.current;
      if (t) t.scrollTop = t.scrollHeight;
    });
    try {
      const { answer, meta } = await aiNoteAsk({
        selection: target.anchor,
        question: q,
        noteMarkdown: body,
        history,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
      });
      const turn: AskTurn = {
        question: q,
        answer,
        createdAt: Date.now(),
        model: meta.model,
      };
      // 디스크 기준 병합 — 요청 중 다른 저장이 있었어도 해당 스레드에만 덧붙인다
      const list = await loadComments(rel);
      const next = list.map((c) =>
        c.id === target.id
          ? { ...c, followUps: [...(c.followUps ?? []), turn] }
          : c,
      );
      await saveComments(rel, next);
      if (relRef.current === rel) {
        setComments(next);
        notifyCount(next);
        // 새로 달린 답변이 보이게 스레드 맨 아래로 + 바로 이어서 물을 수 있게 포커스 복원
        requestAnimationFrame(() => {
          const t = threadRef.current;
          if (t) t.scrollTop = t.scrollHeight;
          followUpRef.current?.focus();
        });
      }
    } catch (e) {
      setAskError(friendlyError(e));
      setQuestion(q); // 실패한 질문은 입력으로 되돌려 바로 다시 보낼 수 있게
    } finally {
      setAsking(false);
      setPendingQ(null);
    }
  }

  /** 이미 나온 답변을 지시문대로 고쳐 **제자리에서 교체**한다(후속 질문처럼 덧붙이지 않는다).
   *  turn 0 = 첫 문답의 answer, 그 뒤는 followUps[turn-1].answer. */
  async function submitRevise(turn: number) {
    const target = viewComment;
    const instruction = reviseText.trim();
    if (!target || !config || asking || instruction.length < 2) return;
    const prev =
      turn === 0 ? target.answer : (target.followUps ?? [])[turn - 1]?.answer;
    if (!prev) return;
    const rel = noteRel;
    setAsking(true);
    setAskError(null);
    try {
      const { answer, meta } = await aiNoteAsk({
        selection: target.anchor,
        question: instruction,
        noteMarkdown: body,
        revise: prev, // 이게 있으면 '새 질문'이 아니라 고쳐쓰기 요청이 된다(context/note-ask.md)
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
      });
      // 디스크 기준 병합 — 요청 중 다른 저장이 있었어도 이 답변만 갈아끼운다
      const list = await loadComments(rel);
      const next = list.map((c) => {
        if (c.id !== target.id) return c;
        if (turn === 0) return { ...c, answer, model: meta.model };
        const ups = [...(c.followUps ?? [])];
        if (!ups[turn - 1]) return c;
        ups[turn - 1] = { ...ups[turn - 1], answer, model: meta.model };
        return { ...c, followUps: ups };
      });
      await saveComments(rel, next);
      if (relRef.current === rel) {
        setComments(next);
        setRevising(null);
        setReviseText("");
      }
    } catch (e) {
      setAskError(friendlyError(e));
    } finally {
      setAsking(false);
    }
  }

  async function deleteComment(id: string) {
    const next = comments.filter((c) => c.id !== id);
    await saveComments(noteRel, next);
    setComments(next);
    notifyCount(next);
    setPop(null);
  }

  // ---- 렌더 (플로팅 요소는 transform 있는 조상 이슈를 피해 body 로 포탈) ----

  const fabStyle = selInfo
    ? {
        left: Math.min(Math.max(12, selInfo.x + 6), window.innerWidth - 96),
        top: Math.max(8, selInfo.y - 38),
      }
    : undefined;

  // 실측한 여백이 있으면 그 자리에(폭 제한), 없으면 CSS 기본값(우측 오버레이 폴백)
  const popStyle = dock
    ? { left: dock.left, width: dock.width, right: "auto" as const }
    : undefined;

  // 목록 순서 = 본문 등장 순서. 읽던 흐름과 같아야 어디에 단 질문인지 바로 잡힌다.
  // 앵커가 사라진 건 위치가 없으니 맨 뒤로 보내고, 그 안에서는 새로 단 순서.
  const listRows = [...comments].sort((a, b) => {
    const pa = anchors.pos.get(a.id) ?? Number.MAX_SAFE_INTEGER;
    const pb = anchors.pos.get(b.id) ?? Number.MAX_SAFE_INTEGER;
    return pa !== pb ? pa - pb : a.createdAt - b.createdAt;
  });

  const viewComment =
    pop?.kind === "view" ? comments.find((c) => c.id === pop.id) : undefined;
  // 스레드 = 첫 문답 + 후속 문답들 (v1 사이드카는 첫 문답 하나)
  const turns: AskTurn[] = viewComment
    ? [
        {
          question: viewComment.question,
          answer: viewComment.answer,
          createdAt: viewComment.createdAt,
          model: viewComment.model,
        },
        ...(viewComment.followUps ?? []),
      ]
    : [];

  return createPortal(
    <>
      {selInfo && !pop && (
        <div className="cmt-fab-bar" style={fabStyle}>
          <button
            className="cmt-fab"
            title={t("notes.cmt.askTip")}
            // mousedown 은 선택 해제 방지만, 열기는 click 에서 (앵커는 이미 캡처됨)
            onMouseDown={(e) => e.preventDefault()}
            onClick={openAsk}
          >
            <Icon name="message" size={13} />
            {t("notes.cmt.ask")}
          </button>
          {onPromote && (
            <button
              className="cmt-fab"
              title={t("notes.cmt.promoteTip")}
              onMouseDown={(e) => e.preventDefault()}
              onClick={openPromote}
            >
              <Icon name="layers" size={13} />
              {t("notes.cmt.promote")}
            </button>
          )}
        </div>
      )}

      {/* 이 노트의 질문 전부 — 본문 등장 순서. 한 줄을 누르면 그 스레드 뷰로 넘어간다.
          앵커가 사라진 질문도 여기선 그냥 한 줄일 뿐이라, 따로 경고를 띄우지 않는다. */}
      {listOpen && !pop && (
        <div ref={listRef} className="cmt-pop cmt-list" style={popStyle}>
          <div className="cmt-pop-head">
            <div className="cmt-list-title">
              <Icon name="message" size={13} />
              {t("notes.qlist.title", { n: comments.length })}
            </div>
            <Tooltip label={t("common.close")}>
              <button
                aria-label={t("common.close")}
                className="icon-btn ghost sm"
                onClick={() => onListOpenChange?.(false)}
              >
                <Icon name="x" size={14} />
              </button>
            </Tooltip>
          </div>
          {comments.length === 0 ? (
            <p className="hint" style={{ margin: 0 }}>
              {t("notes.qlist.empty")}
            </p>
          ) : (
            <div className="cmt-list-rows">
              {listRows.map((cm) => {
                const turnCount = 1 + (cm.followUps?.length ?? 0);
                const last =
                  cm.followUps?.length
                    ? cm.followUps[cm.followUps.length - 1].createdAt
                    : cm.createdAt;
                return (
                  <div className="cmt-list-row" key={cm.id}>
                    <button
                      className="cmt-list-main"
                      onClick={() => openThread(cm.id, true)}
                    >
                      <span className="cmt-list-q">{cm.question}</span>
                      <span className="cmt-anchor">“{cm.anchor}”</span>
                      <span className="cmt-list-meta">
                        {timeAgo(last)}
                        {turnCount > 1 && (
                          <> · {t("notes.qlist.turns", { n: turnCount })}</>
                        )}
                        {anchors.missing.has(cm.id) && (
                          <span
                            className="cmt-list-gone"
                            title={t("notes.qlist.missingTip")}
                          >
                            {t("notes.qlist.missing")}
                          </span>
                        )}
                      </span>
                    </button>
                    <Tooltip label={t("notes.cmt.deleteThread")}>
                      <button
                        className="icon-btn ghost sm danger"
                        aria-label={t("notes.cmt.deleteThread")}
                        onClick={() => void deleteComment(cm.id)}
                      >
                        <Icon name="trash" size={12} />
                      </button>
                    </Tooltip>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {pop && (
        <div ref={popRef} className="cmt-pop" style={popStyle}>
          {pop.kind === "ask" ? (
            <>
              <div className="cmt-pop-head">
                <div className="cmt-anchor" title={pop.anchor}>
                  “{pop.anchor}”
                </div>
                <Tooltip label={t("common.close")}>
                  <button
                    aria-label={t("common.close")}
                    className="icon-btn ghost sm"
                    onClick={() => setPop(null)}
                    disabled={asking}
                  >
                    <Icon name="x" size={14} />
                  </button>
                </Tooltip>
              </div>
              <textarea
                className="textarea"
                style={{ fontFamily: "var(--font)" }}
                rows={2}
                autoFocus
                placeholder={t("notes.cmt.askPh")}
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    void submitAsk();
                  }
                }}
              />
              {askError && (
                <div className="error-note" style={{ marginTop: 8 }}>
                  {askError}
                </div>
              )}
              {asking ? (
                <AiThinking compact label={t("notes.cmt.thinking")} />
              ) : (
                <div className="cmt-actions">
                  <button className="btn btn-sm" onClick={() => setPop(null)}>
                    {t("common.cancel")}
                  </button>
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => void submitAsk()}
                    disabled={question.trim().length < 2 || !config?.provider}
                  >
                    <Icon name="sparkles" size={13} />
                    {t("notes.cmt.askAi")}
                  </button>
                </div>
              )}
            </>
          ) : viewComment ? (
            <>
              <div className="cmt-pop-head">
                {pop.fromList && (
                  <Tooltip label={t("notes.qlist.back")}>
                    <button
                      aria-label={t("notes.qlist.back")}
                      className="icon-btn ghost sm"
                      onClick={() => {
                        setPop(null);
                        onListOpenChange?.(true);
                      }}
                      disabled={asking}
                    >
                      <Icon name="chevron-left" size={14} />
                    </button>
                  </Tooltip>
                )}
                <div className="cmt-anchor" title={viewComment.anchor}>
                  “{viewComment.anchor}”
                </div>
                <Tooltip label={t("common.close")}>
                  <button
                    aria-label={t("common.close")}
                    className="icon-btn ghost sm"
                    onClick={() => setPop(null)}
                  >
                    <Icon name="x" size={14} />
                  </button>
                </Tooltip>
              </div>
              <div className="cmt-thread" ref={threadRef}>
                {turns.map((turnItem, i) => (
                  <div className="cmt-turn" key={i}>
                    <div className="cmt-q">
                      <Icon name="message" size={12} />
                      {turnItem.question}
                    </div>
                    <div className="cmt-a markdown">
                      <Markdown>{turnItem.answer}</Markdown>
                    </div>
                    {/* 고쳐 쓰기 — 답변을 새 문답으로 덧붙이지 않고 **이 자리에서 교체**한다.
                        "한국어로 바꿔줘", "더 짧게" 처럼 나온 답을 다듬는 용도. */}
                    {revising === i ? (
                      <div className="cmt-revise">
                        <textarea
                          className="textarea"
                          style={{ fontFamily: "var(--font)" }}
                          rows={1}
                          autoFocus
                          placeholder={t("notes.cmt.revisePh")}
                          value={reviseText}
                          disabled={asking}
                          onChange={(e) => setReviseText(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.nativeEvent.isComposing) return;
                            if (e.key === "Enter" && !e.shiftKey) {
                              e.preventDefault();
                              void submitRevise(i);
                            }
                            if (e.key === "Escape") {
                              setRevising(null);
                              setReviseText("");
                            }
                          }}
                        />
                        <button
                          className="btn btn-primary cmt-followup-send"
                          title={t("notes.cmt.reviseSend")}
                          onClick={() => void submitRevise(i)}
                          disabled={asking || reviseText.trim().length < 2}
                        >
                          <Icon name="sparkles" size={15} />
                        </button>
                      </div>
                    ) : (
                      <div className="cmt-turn-actions">
                        <button
                          className="cmt-turn-act"
                          onClick={() => {
                            setRevising(i);
                            setReviseText("");
                            setAskError(null);
                          }}
                          disabled={asking || !config?.provider}
                        >
                          <Icon name="pencil" size={11} />
                          {t("notes.cmt.revise")}
                        </button>
                      </div>
                    )}
                    {asking && revising === i && (
                      <AiThinking compact label={t("notes.cmt.revising")} />
                    )}
                  </div>
                ))}
                {pendingQ?.id === viewComment.id && (
                  <div className="cmt-turn">
                    <div className="cmt-q">
                      <Icon name="message" size={12} />
                      {pendingQ.q}
                    </div>
                    <div className="cmt-a">
                      <AiThinking compact label={t("notes.cmt.thinking")} />
                    </div>
                  </div>
                )}
              </div>
              {askError && (
                <div className="error-note" style={{ marginTop: 8 }}>
                  {askError}
                </div>
              )}
              <div className="cmt-followup">
                <textarea
                  ref={followUpRef}
                  className="textarea"
                  style={{ fontFamily: "var(--font)" }}
                  rows={1}
                  autoFocus
                  placeholder={t("notes.cmt.followUpPh")}
                  value={question}
                  disabled={asking}
                  onChange={(e) => setQuestion(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                      e.preventDefault();
                      void submitFollowUp();
                    }
                  }}
                />
                {/* btn-sm(28px) 이 아니라 표준 높이(34px) — 옆 입력창과 같은 높이여야 한다 */}
                <button
                  className="btn btn-primary cmt-followup-send"
                  title={t("notes.cmt.followUpSend")}
                  onClick={() => void submitFollowUp()}
                  disabled={asking || question.trim().length < 2 || !config?.provider}
                >
                  <Icon name="sparkles" size={15} />
                </button>
              </div>
              <div className="cmt-meta">
                {timeAgo(turns[turns.length - 1]?.createdAt ?? viewComment.createdAt)}
                <span className="spacer" />
                <Tooltip label={t("notes.cmt.deleteThread")}>
                  <button
                    aria-label={t("notes.cmt.deleteThread")}
                    className="icon-btn ghost sm danger"
                    onClick={() => void deleteComment(viewComment.id)}
                    disabled={asking}
                  >
                    <Icon name="trash" size={13} />
                  </button>
                </Tooltip>
              </div>
            </>
          ) : null}
        </div>
      )}
    </>,
    document.body,
  );
}
