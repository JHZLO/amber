// 필기노트 AI 작성 모달: 지시 → 백그라운드 실행(lib/noteAiRun) → 프리뷰 → 에디터 초안으로 적용.
// 실행 상태는 이 컴포넌트가 아니라 스토어에 있다 — X·Esc·닫기는 중단이 아니라 **숨기기**이고, 결과는 노트에
// 매달려 검토를 기다린다(노트 위 배너·트리 점·레일 점이 되돌아갈 길). 그래서 닫을 때 확인을 묻지 않는다 —
// 잃는 것이 없다. 파일에 바로 저장하지 않는다 — 적용 후 사용자가 라이브 프리뷰로 확인하고 ⌘S 로 저장 (AI 출력은 초안).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { loadPrompts, type SavedPrompt } from "../lib/prompts";
import { AiThinking, ChoiceChip, DiscardAiModal, Modal, Tooltip } from "../ui";
import { composeInstruction } from "../lib/aiInstruction";
import { PromptPeekModal } from "./PromptPeekModal";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { loadRecentRefDirs, refDirName, rememberRefDir } from "../lib/refDirs";
import { useAiWaitLine } from "../lib/aiWait";
import {
  continueNoteAi,
  dismissNoteAi,
  getNoteAiRun,
  setNoteAiResult,
  startNoteAi,
  stopNoteAi,
  useNoteAiRun,
} from "../lib/noteAiRun";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

type Step = "prompt" | "loading" | "preview";
type ViewMode = "diff" | "preview" | "source";

// 자주 쓰는 작성 방향 (빈 노트 = 처음부터, 채워진 노트 = 보강)
// 언어는 페이지 로드 시 고정이라 모듈 상수에서 t() 호출해도 안전 (lib/i18n.ts)
const PRESETS = [
  t("notes.ai.preset1"),
  t("notes.ai.preset2"),
  t("notes.ai.preset3"),
  t("notes.ai.preset4"),
  t("notes.ai.preset5"),
];

export function NoteAiModal({
  open,
  path,
  title,
  currentBody,
  config,
  onClose,
  onApplied,
}: {
  open: boolean;
  /** vault 상대 경로 — 실행 스토어의 키 */
  path: string;
  title: string;
  currentBody: string;
  config: AppConfig | null;
  onClose: () => void;
  onApplied: (markdown: string) => void;
}) {
  // 이 노트의 실행(없으면 undefined). 진행·결과·실패 전부 여기서 온다
  const run = useNoteAiRun(path);
  const [instruction, setInstruction] = useState("");
  // 체크한 저장 프롬프트(`s:<id>`)·빠른 지시(`p:<index>`) — 텍스트는 보낼 때 합친다
  const [chosen, setChosen] = useState<Set<string>>(() => new Set());
  // 내용 보기 모달에 띄운 저장 프롬프트 (null = 닫힘)
  const [peek, setPeek] = useState<SavedPrompt | null>(null);
  // 참고 폴더 — 최근 목록(칩)과 그중 이번 요청에 붙일 것. AI 가 읽기 전용으로 살펴본다
  const [refDirs, setRefDirs] = useState<string[]>([]);
  const [refOn, setRefOn] = useState<Set<string>>(() => new Set());
  // "다시 지시" — 결과가 있어도 지시 화면을 보인다. 결과는 새 결과가 올 때까지 스토어에 남는다
  const [revise, setRevise] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("preview");
  const [saved, setSaved] = useState<SavedPrompt[]>([]);
  const streamRef = useRef<HTMLPreElement>(null);

  // 편집(기존 내용 있음) vs 새로 작성 구분 — diff 는 기존 내용이 있을 때만 의미
  const hasExisting = currentBody.trim().length > 0;
  const hasExistingRef = useRef(hasExisting);
  hasExistingRef.current = hasExisting;

  // 화면 단계는 스토어 상태에서 나온다. 실패했어도 이전 결과가 있으면 그 결과 위에 에러를 얹어 보인다
  const step: Step =
    !run || revise
      ? "prompt"
      : run.phase === "running"
        ? "loading"
        : run.phase === "done" || run.result
          ? "preview"
          : "prompt";
  const error = run && !revise && run.phase === "error" ? run.error : null;
  const resultMd = run?.result ?? "";
  const streamText = run?.stream ?? "";

  // 열 때마다 지시 화면을 되살린다 — 이 노트에 실행이 있으면 그때 보낸 지시·칩·폴더를, 없으면 빈칸.
  // 저장 프롬프트는 최신으로 다시 읽는다
  useEffect(() => {
    if (!open) return;
    const cur = getNoteAiRun(path);
    setRevise(false);
    setPeek(null);
    setConfirmDiscard(false);
    setRefDirs(loadRecentRefDirs());
    setInstruction(cur?.typed ?? "");
    setChosen(new Set(cur?.chosen ?? []));
    setRefOn(new Set(cur?.refDirs ?? []));
    setViewMode(cur?.result && hasExistingRef.current ? "diff" : "preview");
    loadPrompts().then(setSaved);
  }, [open, path]);

  // 결과가 도착하면(모달이 떠 있든 아니든) 기존 노트면 변경점(diff)부터, 새 작성이면 미리보기
  const finishedAt = run?.finishedAt ?? null;
  const phase = run?.phase;
  useEffect(() => {
    if (phase === "done") setViewMode(hasExistingRef.current ? "diff" : "preview");
  }, [finishedAt, phase]);

  // 생성 중 새 텍스트가 오면 스트림 박스를 맨 아래로 자동 스크롤
  useEffect(() => {
    if (step === "loading" && streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [streamText, step]);

  function toggle(key: string) {
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

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
    startedAt: run?.startedAt ?? null,
    activity: run?.activity ?? null,
    activityAt: run?.activityAt ?? 0,
    hasRefDirs: (run?.refDirs.length ?? 0) > 0,
  });

  // 텍스트가 있는 프롬프트만 칩으로 (설정에서 추가만 하고 비워둔 것 제외)
  const savedUsable = saved.filter((p) => p.text.trim());
  // 체크한 지시는 입력칸에 붙이지 않고 보낼 때 합친다 — 내가 친 말 → 저장 프롬프트 → 빠른 지시
  const extras = [
    ...savedUsable.filter((p) => chosen.has(`s:${p.id}`)).map((p) => p.text),
    ...PRESETS.filter((_, i) => chosen.has(`p:${i}`)),
  ];
  const finalInstruction = composeInstruction(instruction, extras);
  const tooShort = finalInstruction.length < 2;

  // 실행을 시작한 뒤 노트가 바뀌었나 — 변경점은 지금 내용과 비교하니 그 사실만 알린다
  const baseChanged = !!run?.result && run.baseMarkdown.trim() !== currentBody.trim();

  function runAi() {
    if (!config || tooShort) return;
    setRevise(false);
    void startNoteAi({
      path,
      title,
      markdown: currentBody,
      typed: instruction,
      chosen: [...chosen],
      refDirs: chosenDirs,
      instruction: finalInstruction,
      config,
    });
  }
  function apply() {
    if (!run?.result) return;
    onApplied(run.result);
    dismissNoteAi(path); // 에디터로 들어갔으니 대기 중 결과는 끝
    onClose();
  }
  function discard() {
    dismissNoteAi(path); // run 이 사라져 지시 화면으로 돌아간다
    setConfirmDiscard(false);
  }

  // diff 안내 문구 — 언어별 어순이 달라 "{apply}" 자리에 <b>버튼 라벨</b>을 끼워 넣는다
  const diffHint = t("notes.ai.diffHint").split("{apply}");

  let footer: ReactNode = null;
  if (step === "prompt") {
    footer = (
      <>
        {revise && run?.result && (
          <button className="btn btn-sm" onClick={() => setRevise(false)}>
            <Icon name="chevron-left" size={14} />
            {t("notes.ai.backToResult")}
          </button>
        )}
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={runAi}
          disabled={tooShort || !config?.provider}
          title={!config ? t("notes.ai.configLoading") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("notes.ai.run")}
        </button>
      </>
    );
  } else if (step === "loading") {
    footer = (
      <>
        {/* 5분짜리 실행에 탈출구가 없으면 앱을 끄는 것 말고 방법이 없다 */}
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
  } else if (step === "preview") {
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
        <button className="btn btn-primary" onClick={apply} disabled={!run?.result}>
          <Icon name="check" size={15} />
          {hasExisting ? t("notes.ai.applyDiff") : t("notes.ai.applyNew")}
        </button>
      </>
    );
  }

  return (
    <>
    <Modal open={open} title={t("notes.ai.title")} onClose={onClose} footer={footer} wide>
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "prompt" && (
        <>
          <div className="field">
            <label>{t("notes.ai.instructionLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={4}
              placeholder={t("notes.ai.instructionPh")}
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">{t("notes.ai.hint")}</div>
          </div>
          {savedUsable.length > 0 && (
            <div className="field">
              <label>{t("notes.ai.savedPrompts")}</label>
              <div className="chip-row">
                {savedUsable.map((p) => (
                  <ChoiceChip
                    key={p.id}
                    label={p.label.trim() || p.text.slice(0, 20)}
                    on={chosen.has(`s:${p.id}`)}
                    onToggle={() => toggle(`s:${p.id}`)}
                    icon="sparkles"
                    peek={{ label: t("notes.ai.promptPeek.open"), onOpen: () => setPeek(p) }}
                  />
                ))}
              </div>
            </div>
          )}
          <div className="field">
            <label>{t("notes.ai.presets")}</label>
            <div className="chip-row">
              {PRESETS.map((p, i) => (
                <ChoiceChip
                  key={p}
                  label={p}
                  on={chosen.has(`p:${i}`)}
                  onToggle={() => toggle(`p:${i}`)}
                />
              ))}
            </div>
            {extras.length > 0 && (
              <div className="hint">{t("common.ai.chosenCount", { n: extras.length })}</div>
            )}
          </div>
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
        </>
      )}

      {step === "loading" && (
        <div className="note-stream">
          <AiThinking
            compact={!!streamText}
            label={run?.continuing ? t("notes.ai.continuing") : t("notes.ai.writing")}
            activity={waitLine ?? undefined}
          />
          {streamText && (
            <pre className="note-stream-body" ref={streamRef}>
              {streamText}
              <span className="stream-caret" />
            </pre>
          )}
          {/* 닫아도 계속 쓴다는 걸 여기서 말해 둔다 — 닫기 버튼 라벨만으로는 "취소" 로 읽힌다 */}
          <div className="hint">{t("notes.ai.bg.hint")}</div>
        </div>
      )}

      {step === "preview" && run?.truncated && (
        <div className="warn-note ai-truncated" style={{ marginBottom: 12 }}>
          <span>{t("notes.ai.truncated")}</span>
          <button
            className="btn btn-sm"
            onClick={() => {
              if (config) void continueNoteAi(path, config);
            }}
            disabled={!config}
          >
            <Icon name="sparkles" size={13} />
            {t("notes.ai.continue")}
          </button>
        </div>
      )}
      {step === "preview" && run?.continued && !run.truncated && (
        <div className="warn-note" style={{ marginBottom: 12 }}>
          {t("notes.ai.continued")}
        </div>
      )}
      {step === "preview" && baseChanged && (
        <div className="warn-note" style={{ marginBottom: 12 }}>
          {t("notes.ai.baseChanged")}
        </div>
      )}
      {step === "preview" && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {hasExisting ? t("notes.ai.resultEdited") : t("notes.ai.resultNew")}
            <span className="spacer" />
            <div className="segmented">
              {hasExisting && (
                <button
                  className={`tab ${viewMode === "diff" ? "active" : ""}`}
                  onClick={() => setViewMode("diff")}
                >
                  {t("notes.ai.tabDiff")}
                </button>
              )}
              <button
                className={`tab ${viewMode === "preview" ? "active" : ""}`}
                onClick={() => setViewMode("preview")}
              >
                {t("notes.ai.tabPreview")}
              </button>
              <button
                className={`tab ${viewMode === "source" ? "active" : ""}`}
                onClick={() => setViewMode("source")}
              >
                {t("notes.ai.tabSource")}
              </button>
            </div>
          </label>
          {viewMode === "source" ? (
            <textarea
              className="textarea"
              rows={18}
              value={resultMd}
              onChange={(e) => setNoteAiResult(path, e.target.value)}
            />
          ) : viewMode === "diff" ? (
            <DiffView oldText={currentBody} newText={resultMd} />
          ) : (
            <div className="markdown md-preview">
              <Markdown>{resultMd}</Markdown>
            </div>
          )}
          {hasExisting && (
            <div className="hint">
              {diffHint[0]}
              <b>{t("notes.ai.applyDiff")}</b>
              {diffHint[1]}
            </div>
          )}
        </div>
      )}
    </Modal>
    {/* 저장 프롬프트 내용 보기 — 작성 모달의 형제로 겹쳐 뜬다(Esc 는 위 것만 닫는다) */}
    <PromptPeekModal
      prompt={peek}
      included={peek ? chosen.has(`s:${peek.id}`) : false}
      onToggle={() => {
        if (peek) toggle(`s:${peek.id}`);
      }}
      onClose={() => setPeek(null)}
    />
    {/* 버리기만 확인한다 — 닫기는 아무것도 잃지 않으니 묻지 않는다 */}
    <DiscardAiModal
      open={confirmDiscard}
      mode="discard"
      onKeep={() => setConfirmDiscard(false)}
      onDiscard={discard}
    />
    </>
  );
}
