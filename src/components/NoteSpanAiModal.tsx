// 노트의 **한 조각만** AI 로 고쳐 쓰는 모달. 전문 재작성(NoteAiModal)과 나뉘어 있는 이유는 비용이다:
// 출력은 순차 생성이라 길이가 곧 대기 시간인데, 문단 하나를 고칠 때도 노트 전문을 다시 받으면
// 수 분이 걸린다. 여기서는 노트 전문을 **참고 입력**으로만 보내고 조각만 받아 원래 자리에 끼운다.
//
// 두 가지 경로로 들어온다:
//   selection — 편집 모드 원문(textarea)에서 드래그한 구간. selectionStart/End 가 곧 소스
//               좌표라 되끼울 위치를 따로 찾을 필요가 없다.
//   section   — 제목 기준으로 쪼갠 절 하나(mdSections). 읽기 모드에서도 쓸 수 있다.

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { aiCancel, aiNoteEditSpanStream, friendlyError, newCancelKey } from "../lib/ai";
import { splitSections, spliceSpan } from "../lib/mdSections";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

type Step = "pick" | "prompt" | "loading" | "preview";
type ViewMode = "diff" | "source";

/** 고칠 조각 = 소스 구간 + 화면에 보여줄 이름 */
interface Target {
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
  /** 조각이 끼워진 **전문**을 넘긴다 — 호출한 쪽은 초안에 그대로 반영하면 된다 */
  onApplied: (nextBody: string) => void;
}) {
  const [step, setStep] = useState<Step>("prompt");
  const [target, setTarget] = useState<Target | null>(null);
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState("");
  const [streamText, setStreamText] = useState("");
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
    setResult("");
    setStreamText("");
    setViewMode("diff");
    if (mode === "selection" && selection && selection.end > selection.start) {
      setTarget({
        kind: "selection",
        start: selection.start,
        end: selection.end,
        label: t("notes.spanAi.selectionLabel"),
      });
      setStep("prompt");
    } else {
      setTarget(null);
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

  const span = target ? body.slice(target.start, target.end) : "";

  async function run() {
    if (!config || !target || instruction.trim().length < 2) return;
    setError(null);
    setStreamText("");
    setStep("loading");
    const key = newCancelKey();
    cancelKey.current = key;
    const my = ++runSeq.current;
    try {
      const { text } = await aiNoteEditSpanStream(
        {
          title,
          markdown: body,
          span,
          instruction,
          spanKind: target.kind,
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
      setResult(text);
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
    runSeq.current++;
    cancelKey.current = null;
    if (k) void aiCancel(k);
    setStreamText("");
    setStep("prompt");
  }

  const tooShort = instruction.trim().length < 2;

  let footer: ReactNode = null;
  if (step === "pick") {
    footer = (
      <button className="btn btn-sm" onClick={onClose}>
        {t("common.cancel")}
      </button>
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
          disabled={tooShort || !config?.provider || !target}
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
          onClick={() => {
            if (!target) return;
            onApplied(spliceSpan(body, target.start, target.end, result));
            onClose();
          }}
          disabled={!result.trim()}
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
          <label>{t("notes.spanAi.pickLabel")}</label>
          {sections.length === 0 ? (
            <p className="hint" style={{ margin: 0 }}>
              {t("notes.spanAi.noSections")}
            </p>
          ) : (
            <>
              <div className="span-pick">
                {sections.map((s) => (
                  <button
                    key={`${s.start}`}
                    className={`span-pick-row lv${s.level}`}
                    onClick={() => {
                      setTarget({
                        kind: "section",
                        start: s.start,
                        end: s.end,
                        label: s.title,
                      });
                      setStep("prompt");
                    }}
                  >
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

      {(step === "prompt" || step === "loading") && target && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {target.label}
            <span className="spacer" />
            <span className="hint" style={{ margin: 0 }}>
              {t("notes.spanAi.chars", { n: span.length.toLocaleString() })}
            </span>
          </label>
          <pre className="span-source">{span}</pre>
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

      {step === "loading" && (
        <div className="note-stream">
          <AiThinking
            compact={!!streamText}
            label={t("notes.spanAi.editing")}
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
          {viewMode === "diff" ? (
            <DiffView oldText={span} newText={result} />
          ) : (
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--mono)" }}
              rows={14}
              value={result}
              onChange={(e) => setResult(e.target.value)}
            />
          )}
          <div className="hint">{t("notes.spanAi.applyHint")}</div>
        </div>
      )}
    </Modal>
  );
}
