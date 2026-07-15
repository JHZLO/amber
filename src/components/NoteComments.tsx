// 필기노트 인라인 질문(노션 댓글식) 레이어 — 읽기 모드 전용.
// 드래그 선택 → "질문" 플로팅 버튼 → 팝오버에서 질문 → AI 짧은 답변 → 사이드카에 저장.
// 저장된 질문은 본문 텍스트에 하이라이트(CSS Custom Highlight API, DOM 무변경)로 표시되고
// 클릭하면 팝오버로 질문/답변을 본다. 앵커 = "렌더된 텍스트 문자열 + n번째 출현".

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { AppConfig } from "../lib/config";
import { claudeNoteAsk, friendlyError } from "../lib/claude";
import {
  loadComments,
  newCommentId,
  saveComments,
  type NoteComment,
} from "../lib/comments";
import { Markdown } from "./Markdown";
import { timeAgo } from "../ui";
import { Icon } from "../icons";

const HIGHLIGHT_KEY = "note-q";
const POP_W = 380;

type Pop =
  | { kind: "ask"; x: number; y: number; anchor: string; occurrence: number }
  | { kind: "view"; x: number; y: number; id: string };

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

/** anchor 의 occurrence 번째 출현을 Range 로 복원 (없으면 null) */
function findNthRange(
  container: HTMLElement,
  anchor: string,
  occurrence: number,
): Range | null {
  if (!anchor) return null;
  const fullText = container.textContent ?? "";
  let idx = -1;
  for (let i = 0; i <= occurrence; i++) {
    idx = fullText.indexOf(anchor, idx + 1);
    if (idx === -1) return null;
  }
  const endIdx = idx + anchor.length;
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
}: {
  noteRel: string;
  body: string;
  containerRef: RefObject<HTMLDivElement | null>;
  config: AppConfig | null;
  onCountChange?: (n: number) => void;
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

  const popRef = useRef<HTMLDivElement>(null);
  const rangesRef = useRef<{ id: string; range: Range }[]>([]);
  const relRef = useRef(noteRel);
  relRef.current = noteRel;

  const notifyCount = useCallback(
    (list: NoteComment[]) => onCountChange?.(list.length),
    [onCountChange],
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

  // 하이라이트: 렌더된 DOM 에서 앵커 복원 → CSS Custom Highlight (미지원이면 표시만 생략)
  useEffect(() => {
    const c = containerRef.current;
    rangesRef.current = [];
    const registry = highlightRegistry();
    if (!c || !registry) return;
    const HL = (
      window as unknown as { Highlight: new (...r: Range[]) => unknown }
    ).Highlight;
    const found: { id: string; range: Range }[] = [];
    for (const cm of comments) {
      // 노트가 수정돼 n번째 출현이 사라졌으면 첫 출현으로 폴백
      const r =
        findNthRange(c, cm.anchor, cm.occurrence) ??
        findNthRange(c, cm.anchor, 0);
      if (r) found.push({ id: cm.id, range: r });
    }
    rangesRef.current = found;
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
            setPop({ kind: "view", x: e.clientX, y: e.clientY, id });
            return;
          }
        } catch {
          /* 노드가 교체된 stale range — 무시 */
        }
      }
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [containerRef]);

  // 팝오버 닫기: 바깥 클릭 / Esc / 스크롤 (답변 생성 중엔 유지)
  useEffect(() => {
    if (!pop) return;
    const down = (e: MouseEvent) => {
      if (popRef.current && !popRef.current.contains(e.target as Node))
        setPop(null);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !asking) setPop(null);
    };
    // 스크롤 시엔 '보기' 팝오버만 닫는다 — '질문 작성'은 autoFocus 가 유발하는
    // 미세 스크롤에 곧바로 닫히는 경합이 있어 열어 둔다(fixed 라 위치는 유지됨).
    const scroll = () => {
      setPop((p) => (p && p.kind === "view" ? null : p));
    };
    document.addEventListener("mousedown", down);
    window.addEventListener("keydown", key);
    window.addEventListener("scroll", scroll, true);
    return () => {
      document.removeEventListener("mousedown", down);
      window.removeEventListener("keydown", key);
      window.removeEventListener("scroll", scroll, true);
    };
  }, [pop, asking]);

  // "질문" 버튼: 선택 시점에 계산해 둔 정보로 팝오버를 연다 (라이브 셀렉션에 의존하지 않음)
  function openAsk() {
    const si = selInfo;
    if (!si) return;
    setQuestion("");
    setAskError(null);
    setPop({
      kind: "ask",
      x: si.x,
      y: si.bottom,
      anchor: si.anchor,
      occurrence: si.occurrence,
    });
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
      const { answer, meta } = await claudeNoteAsk({
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
        setPop((p) =>
          p && p.kind === "ask" ? { kind: "view", x: p.x, y: p.y, id: cm.id } : p,
        );
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

  const popStyle = pop
    ? {
        left: Math.min(Math.max(12, pop.x - 40), window.innerWidth - POP_W - 12),
        top: Math.min(pop.y + 10, window.innerHeight - 280),
      }
    : undefined;

  const viewComment =
    pop?.kind === "view" ? comments.find((c) => c.id === pop.id) : undefined;

  return createPortal(
    <>
      {selInfo && !pop && (
        <button
          className="cmt-fab"
          style={fabStyle}
          title="선택한 부분에 질문 달기"
          // mousedown 은 선택 해제 방지만, 열기는 click 에서 (앵커는 이미 캡처됨)
          onMouseDown={(e) => e.preventDefault()}
          onClick={openAsk}
        >
          <Icon name="message" size={13} />
          질문
        </button>
      )}

      {pop && (
        <div ref={popRef} className="cmt-pop" style={popStyle}>
          {pop.kind === "ask" ? (
            <>
              <div className="cmt-anchor" title={pop.anchor}>
                “{pop.anchor}”
              </div>
              <textarea
                className="textarea"
                style={{ fontFamily: "var(--font)" }}
                rows={2}
                autoFocus
                placeholder="이 부분에서 무엇이 궁금한가요?"
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
              <div className="cmt-actions">
                <button
                  className="btn btn-sm"
                  onClick={() => setPop(null)}
                  disabled={asking}
                >
                  취소
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => void submitAsk()}
                  disabled={asking || question.trim().length < 2 || !config?.provider}
                >
                  <Icon name="sparkles" size={13} />
                  {asking ? "답변 생성 중…" : "AI에게 질문"}
                </button>
              </div>
            </>
          ) : viewComment ? (
            <>
              <div className="cmt-anchor" title={viewComment.anchor}>
                “{viewComment.anchor}”
              </div>
              <div className="cmt-q">
                <Icon name="message" size={12} />
                {viewComment.question}
              </div>
              <div className="cmt-a markdown">
                <Markdown>{viewComment.answer}</Markdown>
              </div>
              <div className="cmt-meta">
                {timeAgo(viewComment.createdAt)}
                <span className="spacer" />
                <button
                  className="icon-btn ghost sm danger"
                  title="질문 삭제"
                  onClick={() => void deleteComment(viewComment.id)}
                >
                  <Icon name="trash" size={13} />
                </button>
              </div>
            </>
          ) : null}
        </div>
      )}
    </>,
    document.body,
  );
}
