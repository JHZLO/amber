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
// 고치면 이음새 문장이 서로 어긋난다. 떨어진 묶음은 각각 따로 호출한다.
//
// **실행은 이 컴포넌트가 들고 있지 않다.** lib/noteAiRun 스토어가 전문 작성과 같은 자리에서 돌린다 —
// 모달을 닫아도 계속 돌고, 노트 위 배너·트리 점으로 돌아올 길이 남는다. 조각이라 금방 끝난다고 봤던
// 게 틀렸다: CLI 를 깨우는 시간은 조각 길이와 무관해서 한 문단에도 수십 초가 걸린다.

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { mergeRuns, splitSections } from "../lib/mdSections";
import {
  dismissNoteAi,
  getNoteAiRun,
  setNoteSpanResult,
  startNoteSpanAi,
  stopNoteAi,
  useNoteAiRun,
  type NoteAiSpan,
} from "../lib/noteAiRun";
import { AiThinking, ChoiceChip, DiscardAiModal, Modal, Tooltip } from "../ui";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { loadRecentRefDirs, refDirName, rememberRefDir } from "../lib/refDirs";
import { useAiWaitLine } from "../lib/aiWait";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

type Step = "pick" | "prompt" | "loading" | "preview";
type ViewMode = "diff" | "source";

/** 한 번의 호출로 고칠 덩어리 = 소스 구간 + 화면에 보여줄 이름 (스토어와 같은 모양) */
type Run = NoteAiSpan;

export function NoteSpanAiModal({
  open,
  mode,
  path,
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
  /** vault 상대 경로 — 실행 스토어의 키 */
  path: string;
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
  // 이 노트의 실행(없으면 undefined). 진행·결과·실패 전부 스토어에서 온다
  const run = useNoteAiRun(path);
  const spanRun = run?.kind === "span" ? run : undefined;
  /** 지시·검토를 오갈 때의 화면 상태. 실행 중/결과 있음은 스토어가 정한다 */
  const [stage, setStage] = useState<"pick" | "prompt">("prompt");
  const [runs, setRuns] = useState<Run[]>([]);
  /** 고른 절의 인덱스 (splitSections 결과 기준) */
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [instruction, setInstruction] = useState("");
  // 참고 폴더 — 최근 목록(칩)과 그중 이번 요청에 붙일 것. AI 가 읽기 전용으로 살펴본다
  const [refDirs, setRefDirs] = useState<string[]>([]);
  const [refOn, setRefOn] = useState<Set<string>>(() => new Set());
  // "다시 지시" — 결과가 있어도 지시 화면을 보인다. 결과는 새 결과가 올 때까지 스토어에 남는다
  const [revise, setRevise] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("diff");
  const streamRef = useRef<HTMLPreElement>(null);

  // 화면 단계는 스토어 상태에서 나온다. 실패했어도 이전 결과가 있으면 그 결과 위에 에러를 얹어 보인다
  const step: Step =
    !spanRun || revise
      ? stage
      : spanRun.phase === "running"
        ? "loading"
        : spanRun.phase === "done" || spanRun.result
          ? "preview"
          : stage;
  const error = spanRun && !revise && spanRun.phase === "error" ? spanRun.error : null;
  const streamText = spanRun?.stream ?? "";
  const results = spanRun?.spanResults ?? [];
  const truncated = spanRun?.truncated ?? false;
  const runAt = spanRun?.spanAt ?? 0;
  // 검토는 실행을 시작한 시점의 본문과 견준다 — 그 사이 노트를 고쳤어도 변경점이 흔들리지 않게
  const baseBody = spanRun?.baseMarkdown ?? body;

  const sections = useMemo(() => (open ? splitSections(body) : []), [open, body]);

  // 열 때마다: 이 노트에 부분 수정 실행이 남아 있으면 그때의 구간·지시·폴더를 되살리고,
  // 없으면 새로 시작한다(selection 은 받은 구간으로 바로, section 은 고르는 화면부터).
  useEffect(() => {
    if (!open) return;
    const cur = getNoteAiRun(path);
    const prior = cur?.kind === "span" ? cur : undefined;
    setRevise(false);
    setConfirmDiscard(false);
    setRefDirs(loadRecentRefDirs());
    setViewMode("diff");
    setPicked(new Set());
    if (prior) {
      setRuns(prior.spans);
      setInstruction(prior.typed);
      setRefOn(new Set(prior.refDirs));
      setStage("prompt");
      return;
    }
    setInstruction("");
    setRefOn(new Set());
    if (mode === "selection" && selection && selection.end > selection.start) {
      setRuns([
        {
          kind: "selection",
          start: selection.start,
          end: selection.end,
          label: t("notes.spanAi.selectionLabel"),
        },
      ]);
      setStage("prompt");
    } else {
      setRuns([]);
      setStage("pick");
    }
    // selection 은 열 때의 값만 쓴다 — 열려 있는 동안 원문 선택이 바뀌어도 대상은 고정이다
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, mode, path]);

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

  async function pickRefDir() {
    // 워크스페이스 루트 전환기와 같은 네이티브 폴더 다이얼로그
    const dir = await openDialog({
      directory: true,
      multiple: false,
      title: t("notes.ai.refDirs.dialogTitle"),
    });
    if (typeof dir !== "string" || !dir) return;
    setRefDirs(rememberRefDir(dir));
    setRefOn((prev) => new Set(prev).add(dir));
  }
  function toggleRef(dir: string) {
    setRefOn((prev) => {
      const next = new Set(prev);
      if (next.has(dir)) next.delete(dir);
      else next.add(dir);
      return next;
    });
  }
  const chosenDirs = refDirs.filter((d) => refOn.has(d));
  const waitLine = useAiWaitLine({
    running: step === "loading",
    startedAt: spanRun?.startedAt ?? null,
    activity: spanRun?.activity ?? null,
    activityAt: spanRun?.activityAt ?? 0,
    hasRefDirs: (spanRun?.refDirs.length ?? 0) > 0,
  });

  /** 실행 시작 — 컴포넌트가 아니라 스토어가 돌린다. 닫아도 끝까지 간다 */
  function start() {
    if (!config || runs.length === 0 || instruction.trim().length < 2) return;
    setRevise(false);
    void startNoteSpanAi({
      path,
      title,
      markdown: body,
      spans: runs,
      instruction,
      refDirs: chosenDirs,
      config,
    });
  }

  /** 되끼운 전문을 넘긴다 — 스토어가 조각을 뒤에서부터 끼워 이미 만들어 뒀다 */
  function apply() {
    if (!spanRun?.result) return;
    onApplied(spanRun.result);
    dismissNoteAi(path); // 에디터로 들어갔으니 대기 중 결과는 끝
    onClose();
  }
  function discard() {
    dismissNoteAi(path);
    setConfirmDiscard(false);
  }

  // 실행을 시작한 뒤 노트가 바뀌었나 — 되끼운 전문이 그 편집을 덮어쓰게 되므로 알린다
  const baseChanged = !!spanRun?.result && spanRun.baseMarkdown !== body;
  // 노트 하나에 실행 하나다. 전문 작성이 도는 중이면 시작이 조용히 무시되므로 먼저 말해 준다
  const composeBusy = run?.kind === "compose" && run.phase === "running";

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
            setStage("prompt");
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
        {revise && spanRun?.result ? (
          <button className="btn btn-sm" onClick={() => setRevise(false)}>
            <Icon name="chevron-left" size={14} />
            {t("notes.ai.backToResult")}
          </button>
        ) : (
          mode === "section" && (
            <button className="btn btn-sm" onClick={() => setStage("pick")}>
              <Icon name="chevron-left" size={14} />
              {t("notes.spanAi.backToPick")}
            </button>
          )
        )}
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={start}
          disabled={tooShort || !config?.provider || runs.length === 0 || composeBusy}
          title={!config ? t("notes.ai.configLoading") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("notes.spanAi.run")}
        </button>
      </>
    );
  } else if (step === "loading") {
    footer = (
      <>
        {/* 탈출구는 남기되, 닫기는 중단이 아니다 — 스토어가 계속 돌린다 */}
        <button className="btn btn-sm btn-danger-ghost" onClick={() => stopNoteAi(path)}>
          <Icon name="x" size={14} />
          {t("notes.ai.stop")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("notes.ai.bg.keep")}
        </button>
      </>
    );
  } else {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setRevise(true)}>
          <Icon name="chevron-left" size={14} />
          {t("notes.ai.back")}
        </button>
        <button className="btn btn-sm btn-danger-ghost" onClick={() => setConfirmDiscard(true)}>
          {t("notes.ai.discard")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.close")}
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
    <>
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
      {composeBusy && (
        <div className="warn-note" style={{ marginBottom: 12 }}>
          {t("notes.spanAi.composeBusy")}
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
                // 항상 compact — 고칠 원문 바로 아래 붙는 한 줄이라, 가운데 정렬된 큰 블록을 쓰면
                // 원문과 진행 표시 사이가 벌어져 모달이 텅 빈 것처럼 읽힌다
                <div className="note-stream" style={{ marginTop: 10 }}>
                  <AiThinking
                    compact
                    activity={waitLine ?? undefined}
                    label={
                      runs.length > 1
                        ? t("notes.spanAi.progress", {
                            i: runAt + 1,
                            n: runs.length,
                          })
                        : t("notes.spanAi.editing")
                    }
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
          {step === "loading" && <div className="hint">{t("notes.ai.bg.hint")}</div>}
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
                start();
              }
            }}
          />
          <div className="hint">{t("notes.spanAi.hint")}</div>
        </div>
      )}

      {step === "prompt" && (
        <div className="field">
          <label>{t("notes.ai.refDirs.label")}</label>
          <div className="chip-row">
            <button type="button" className="btn btn-sm" onClick={() => void pickRefDir()}>
              <Icon name="folder-plus" size={13} />
              {t("notes.ai.refDirs.add")}
            </button>
            {refDirs.map((d) => (
              <Tooltip key={d} label={d}>
                <ChoiceChip
                  label={refDirName(d)}
                  icon="folder"
                  on={refOn.has(d)}
                  onToggle={() => toggleRef(d)}
                />
              </Tooltip>
            ))}
          </div>
          <div className="hint">{t("notes.ai.refDirs.hint")}</div>
        </div>
      )}

      {step === "preview" && baseChanged && (
        <div className="warn-note" style={{ marginBottom: 12 }}>
          {t("notes.spanAi.baseChanged")}
        </div>
      )}
      {step === "preview" && truncated && (
        <div className="warn-note" style={{ marginBottom: 12 }}>
          {t("notes.ai.truncated")}
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
                  oldText={baseBody.slice(r.start, r.end)}
                  newText={results[i] ?? ""}
                />
              ) : (
                <textarea
                  className="textarea"
                  style={{ fontFamily: "var(--mono)" }}
                  rows={runs.length > 1 ? 8 : 14}
                  value={results[i] ?? ""}
                  onChange={(e) => setNoteSpanResult(path, i, e.target.value)}
                />
              )}
            </div>
          ))}
          <div className="hint">{t("notes.spanAi.applyHint")}</div>
        </div>
      )}
    </Modal>
    <DiscardAiModal
      open={confirmDiscard}
      mode="discard"
      onKeep={() => setConfirmDiscard(false)}
      onDiscard={discard}
    />
    </>
  );
}
