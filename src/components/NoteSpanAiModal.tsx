// 노트의 **일부만** AI 로 고쳐 쓰는 모달. 전문 재작성(NoteAiModal)과 나뉘어 있는 이유는 비용이다:
// 출력은 순차 생성이라 길이가 곧 대기 시간인데, 문단 하나를 고칠 때도 노트 전문을 다시 받으면
// 수 분이 걸린다. 여기서는 노트 전문을 **참고 입력**으로만 보내고 조각만 받아 원래 자리에 끼운다.
//
// 두 가지 경로로 들어온다:
//   selection — 편집 모드 원문(textarea)에서 드래그한 구간. selectionStart/End 가 곧 소스
//               좌표라 되끼울 위치를 따로 찾을 필요가 없다.
//   section   — 제목 기준으로 쪼갠 절(mdSections). **여러 개 고를 수 있다.**
//
// 절을 여러 개 고르면 붙어 있는 것끼리 한 덩어리(run)로 묶어 한 번에 고친다 — 이어진 절을 따로
// 고치면 이음새 문장이 서로 어긋난다. 떨어진 묶음은 각각 따로 호출하고, 되끼울 때는 **뒤에서부터**
// 넣는다: 앞을 먼저 갈아끼우면 길이가 달라져 뒤 묶음의 오프셋이 밀린다.

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { aiCancel, aiNoteEditSpanStream, friendlyError, newCancelKey } from "../lib/ai";
import { mergeRuns, splitSections, spliceSpan } from "../lib/mdSections";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

type Step = "pick" | "prompt" | "loading" | "preview";
type ViewMode = "diff" | "source";

/** 한 번의 호출로 고칠 덩어리 = 소스 구간 + 화면에 보여줄 이름 */
interface Run {
  kind: "selection" | "section";
  start: number;
  end: number;
  label: string;
}

export function NoteSpanAiModal({
  open,
  mode,
  title,
  body,
  selection,
  config,
  onClose,
  onApplied,
}: {
  open: boolean;
  /** selection = 넘겨받은 구간을 바로 고친다 · section = 절 목록에서 고를 것부터 시작 */
  mode: "selection" | "section";
  title: string;
  /** 노트 전문 (마크다운 소스) */
  body: string;
  /** mode="selection" 일 때의 소스 구간 */
  selection?: { start: number; end: number } | null;
  config: AppConfig | null;
  onClose: () => void;
  /** 조각들이 끼워진 **전문**을 넘긴다 — 호출한 쪽은 초안에 그대로 반영하면 된다 */
  onApplied: (nextBody: string) => void;
}) {
  const [step, setStep] = useState<Step>("prompt");
  const [runs, setRuns] = useState<Run[]>([]);
  /** 고른 절의 인덱스 (splitSections 결과 기준) */
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<string[]>([]);
  const [streamText, setStreamText] = useState("");
  /** 진행 중인 덩어리 번호 (0-based) — 여러 묶음이면 몇 번째인지 보여준다 */
  const [runAt, setRunAt] = useState(0);
  const [viewMode, setViewMode] = useState<ViewMode>("diff");
  const streamRef = useRef<HTMLPreElement>(null);
  const cancelKey = useRef<string | null>(null);
  // 실행 세대 — 중단·재실행으로 버려진 실행의 델타가 새 버퍼에 섞이지 않게 (NoteAiModal 과 같은 이유)
  const runSeq = useRef(0);

  const sections = useMemo(() => (open ? splitSections(body) : []), [open, body]);

  // 열 때마다 초기화. selection 은 받은 구간으로 바로 시작하고, section 은 고르는 화면부터.
  useEffect(() => {
    if (!open) return;
    runSeq.current++;
    setInstruction("");
    setError(null);
    setResults([]);
    setStreamText("");
    setRunAt(0);
    setViewMode("diff");
    setPicked(new Set());
    if (mode === "selection" && selection && selection.end > selection.start) {
      setRuns([
        {
          kind: "selection",
          start: selection.start,
          end: selection.end,
          label: t("notes.spanAi.selectionLabel"),
        },
      ]);
      setStep("prompt");
    } else {
      setRuns([]);
      setStep("pick");
    }
    // selection 은 열 때의 값만 쓴다 — 열려 있는 동안 원문 선택이 바뀌어도 대상은 고정이다
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, mode]);

  useEffect(() => {
    if (step === "loading" && streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [streamText, step]);

  const pickedChars = useMemo(
    () =>
      [...picked].reduce((sum, i) => {
        const s = sections[i];
        return sum + (s ? s.end - s.start : 0);
      }, 0),
    [picked, sections],
  );

  /** 고른 절 → 붙은 것끼리 묶은 덩어리 목록 */
  function runsFromPicked(): Run[] {
    const spans = [...picked]
      .map((i) => sections[i])
      .filter(Boolean)
      .map((s) => ({ start: s.start, end: s.end, title: s.title }));
    return mergeRuns(spans).map((r) => ({
      kind: "section" as const,
      start: r.start,
      end: r.end,
      // 묶인 절이 여럿이면 처음 … 끝으로 줄인다 (제목을 다 늘어놓으면 한 줄을 넘긴다)
      label:
        r.items.length === 1
          ? r.items[0].title
          : `${r.items[0].title} … ${r.items[r.items.length - 1].title}`,
    }));
  }

  const totalChars = runs.reduce((sum, r) => sum + (r.end - r.start), 0);

  async function run() {
    if (!config || runs.length === 0 || instruction.trim().length < 2) return;
    setError(null);
    setResults([]);
    setStreamText("");
    setRunAt(0);
    setStep("loading");
    const my = ++runSeq.current;
    const out: string[] = [];
    try {
      // 묶음마다 한 번씩. 순차로 도는 이유는 화면이다 — 스트림 박스가 하나라 동시에 흘리면
      // 두 덩어리의 글자가 섞여 무엇을 보고 있는지 알 수 없게 된다.
      for (let i = 0; i < runs.length; i++) {
        if (my !== runSeq.current) return;
        setRunAt(i);
        setStreamText("");
        const key = newCancelKey();
        cancelKey.current = key;
        const { text } = await aiNoteEditSpanStream(
          {
            title,
            markdown: body,
            span: body.slice(runs[i].start, runs[i].end),
            instruction,
            spanKind: runs[i].kind,
            model: config.model,
            cliPath: config.cliPath,
            provider: config.provider,
            cancelKey: key,
          },
          (delta) => {
            if (my !== runSeq.current) return;
            setStreamText((prev) => prev + delta);
          },
        );
        if (my !== runSeq.current) return;
        out.push(text);
      }
      setResults(out);
      setViewMode("diff");
      setStep("preview");
    } catch (e) {
      if (my !== runSeq.current) return;
      setError(friendlyError(e));
      setStep("prompt");
    } finally {
      if (my === runSeq.current) cancelKey.current = null;
    }
  }

  /** 중단 — CLI 를 죽이고 기다리지 않고 지시 화면으로 (NoteAiModal 과 같은 규약) */
  function stop() {
    const k = cancelKey.current;
    runSeq.current++; // 남은 묶음도 여기서 끊긴다
    cancelKey.current = null;
    if (k) void aiCancel(k);
    setStreamText("");
    setStep("prompt");
  }

  /** 되끼우기 — **뒤에서부터**. 앞을 먼저 갈아끼우면 길이가 바뀌어 뒤 오프셋이 밀린다. */
  function apply() {
    let next = body;
    for (let i = runs.length - 1; i >= 0; i--) {
      const text = results[i];
      if (text === undefined) continue;
      next = spliceSpan(next, runs[i].start, runs[i].end, text);
    }
    onApplied(next);
    onClose();
  }

  const tooShort = instruction.trim().length < 2;

  let footer: ReactNode = null;
  if (step === "pick") {
    footer = (
      <>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          disabled={picked.size === 0}
          onClick={() => {
            setRuns(runsFromPicked());
            setStep("prompt");
          }}
        >
          {t("notes.spanAi.next")}
          <Icon name="chevron-right" size={14} />
        </button>
      </>
    );
  } else if (step === "prompt") {
    footer = (
      <>
        {mode === "section" && (
          <button className="btn btn-sm" onClick={() => setStep("pick")}>
            <Icon name="chevron-left" size={14} />
            {t("notes.spanAi.backToPick")}
          </button>
        )}
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={run}
          disabled={tooShort || !config?.provider || runs.length === 0}
          title={!config ? t("notes.ai.configLoading") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("notes.spanAi.run")}
        </button>
      </>
    );
  } else if (step === "loading") {
    footer = (
      <button className="btn btn-sm btn-danger-ghost" onClick={stop}>
        <Icon name="x" size={14} />
        {t("notes.ai.stop")}
      </button>
    );
  } else {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("prompt")}>
          <Icon name="chevron-left" size={14} />
          {t("notes.ai.back")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={apply}
          disabled={!results.some((r) => r.trim())}
        >
          <Icon name="check" size={15} />
          {t("notes.spanAi.apply")}
        </button>
      </>
    );
  }

  return (
    <Modal
      open={open}
      title={t("notes.spanAi.title")}
      onClose={onClose}
      footer={footer}
      wide
    >
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "pick" && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {t("notes.spanAi.pickLabel")}
            <span className="spacer" />
            {picked.size > 0 && (
              <span className="hint" style={{ margin: 0 }}>
                {t("notes.spanAi.pickCount", {
                  n: picked.size,
                  chars: pickedChars.toLocaleString(),
                })}
              </span>
            )}
          </label>
          {sections.length === 0 ? (
            <p className="hint" style={{ margin: 0 }}>
              {t("notes.spanAi.noSections")}
            </p>
          ) : (
            <>
              <div className="span-pick">
                {sections.map((s, i) => (
                  <button
                    key={`${s.start}`}
                    className={`span-pick-row lv${s.level} ${picked.has(i) ? "on" : ""}`}
                    aria-pressed={picked.has(i)}
                    onClick={() =>
                      setPicked((prev) => {
                        const next = new Set(prev);
                        if (next.has(i)) next.delete(i);
                        else next.add(i);
                        return next;
                      })
                    }
                  >
                    <span className="span-pick-box" aria-hidden="true">
                      {picked.has(i) && <Icon name="check" size={11} />}
                    </span>
                    <span className="span-pick-title">{s.title}</span>
                    <span className="span-pick-size">
                      {t("notes.spanAi.chars", {
                        n: (s.end - s.start).toLocaleString(),
                      })}
                    </span>
                  </button>
                ))}
              </div>
              <div className="hint">{t("notes.spanAi.pickHint")}</div>
            </>
          )}
        </div>
      )}

      {(step === "prompt" || step === "loading") && runs.length > 0 && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {runs.length === 1
              ? runs[0].label
              : t("notes.spanAi.runsLabel", { n: runs.length })}
            <span className="spacer" />
            <span className="hint" style={{ margin: 0 }}>
              {t("notes.spanAi.chars", { n: totalChars.toLocaleString() })}
            </span>
          </label>
          {runs.map((r, i) => (
            <div key={r.start} className="span-run">
              {runs.length > 1 && (
                <div className="span-run-head">
                  <span className="span-run-name">{r.label}</span>
                  <span className="span-pick-size">
                    {t("notes.spanAi.chars", {
                      n: (r.end - r.start).toLocaleString(),
                    })}
                  </span>
                </div>
              )}
              <pre className="span-source">{body.slice(r.start, r.end)}</pre>
              {step === "loading" && i === runAt && (
                <div className="note-stream" style={{ marginTop: 8 }}>
                  <AiThinking
                    compact={!!streamText}
                    label={
                      runs.length > 1
                        ? t("notes.spanAi.progress", {
                            i: runAt + 1,
                            n: runs.length,
                          })
                        : t("notes.spanAi.editing")
                    }
                    hint={streamText ? undefined : t("notes.ai.waiting")}
                  />
                  {streamText && (
                    <pre className="note-stream-body" ref={streamRef}>
                      {streamText}
                      <span className="stream-caret" />
                    </pre>
                  )}
                </div>
              )}
            </div>
          ))}
          {runs.length > 1 && step === "prompt" && (
            <div className="hint">{t("notes.spanAi.runsHint", { n: runs.length })}</div>
          )}
        </div>
      )}

      {step === "prompt" && (
        <div className="field">
          <label>{t("notes.spanAi.instructionLabel")}</label>
          <textarea
            className="textarea"
            style={{ fontFamily: "var(--font)" }}
            rows={3}
            autoFocus
            placeholder={t("notes.spanAi.instructionPh")}
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void run();
              }
            }}
          />
          <div className="hint">{t("notes.spanAi.hint")}</div>
        </div>
      )}

      {step === "preview" && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {t("notes.spanAi.resultLabel")}
            <span className="spacer" />
            <div className="segmented">
              <button
                className={`tab ${viewMode === "diff" ? "active" : ""}`}
                onClick={() => setViewMode("diff")}
              >
                {t("notes.ai.tabDiff")}
              </button>
              <button
                className={`tab ${viewMode === "source" ? "active" : ""}`}
                onClick={() => setViewMode("source")}
              >
                {t("notes.ai.tabSource")}
              </button>
            </div>
          </label>
          {runs.map((r, i) => (
            <div key={r.start} className="span-run">
              {runs.length > 1 && (
                <div className="span-run-head">
                  <span className="span-run-name">{r.label}</span>
                </div>
              )}
              {viewMode === "diff" ? (
                <DiffView
                  oldText={body.slice(r.start, r.end)}
                  newText={results[i] ?? ""}
                />
              ) : (
                <textarea
                  className="textarea"
                  style={{ fontFamily: "var(--mono)" }}
                  rows={runs.length > 1 ? 8 : 14}
                  value={results[i] ?? ""}
                  onChange={(e) =>
                    setResults((prev) => {
                      const next = [...prev];
                      next[i] = e.target.value;
                      return next;
                    })
                  }
                />
              )}
            </div>
          ))}
          <div className="hint">{t("notes.spanAi.applyHint")}</div>
        </div>
      )}
    </Modal>
  );
}
