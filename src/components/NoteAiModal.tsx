// 필기노트 AI 작성 모달: 지시 → claude_note_compose → 프리뷰 → 에디터 초안으로 적용.
// 파일에 바로 저장하지 않는다 — 적용 후 사용자가 라이브 프리뷰로 확인하고 ⌘S 로 저장 (AI 출력은 초안).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { aiCancel, aiNoteComposeStream, friendlyError, newCancelKey } from "../lib/ai";
import { loadPrompts, type SavedPrompt } from "../lib/prompts";
import { AiThinking, Modal } from "../ui";
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
  title,
  currentBody,
  config,
  onClose,
  onApplied,
}: {
  open: boolean;
  title: string;
  currentBody: string;
  config: AppConfig | null;
  onClose: () => void;
  onApplied: (markdown: string) => void;
}) {
  const [step, setStep] = useState<Step>("prompt");
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [resultMd, setResultMd] = useState("");
  const [streamText, setStreamText] = useState(""); // 생성 중 실시간 누적 텍스트
  const [viewMode, setViewMode] = useState<ViewMode>("preview");
  const [saved, setSaved] = useState<SavedPrompt[]>([]);
  const streamRef = useRef<HTMLPreElement>(null);
  // 진행 중인 실행의 취소 키 — 중단 버튼이 이걸로 CLI 를 끝낸다
  const cancelKey = useRef<string | null>(null);
  // 실행 세대. 중단·재실행으로 버려진 실행의 델타가 새 버퍼에 섞여 들어가지 않게 한다 —
  // 취소는 비동기라 프로세스가 죽기 전 조각이 더 오고, 그게 새 실행 텍스트와 뒤엉키면
  // "## Met# 서비스 조adata" 처럼 두 생성이 한 글자씩 섞인 결과가 나온다.
  const runSeq = useRef(0);

  // 편집(기존 내용 있음) vs 새로 작성 구분 — diff 는 기존 내용이 있을 때만 의미
  const hasExisting = currentBody.trim().length > 0;

  // 열 때마다 초기화 (닫혀 있는 동안의 stale 상태 방지) + 저장 프롬프트 최신 로드
  useEffect(() => {
    if (!open) return;
    runSeq.current++; // 닫힌 동안 계속 돌던 실행의 델타를 이 세션에서 끊는다
    setStep("prompt");
    setInstruction("");
    setError(null);
    setResultMd("");
    setStreamText("");
    setViewMode("preview");
    loadPrompts().then(setSaved);
  }, [open]);

  // 생성 중 새 텍스트가 오면 스트림 박스를 맨 아래로 자동 스크롤
  useEffect(() => {
    if (step === "loading" && streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [streamText, step]);

  function addPreset(p: string) {
    setInstruction((prev) => (prev.trim() ? `${prev.trim()} · ${p}` : p));
  }

  async function run() {
    if (!config || instruction.trim().length < 2) return;
    setError(null);
    setStreamText("");
    setStep("loading");
    const key = newCancelKey();
    cancelKey.current = key;
    const my = ++runSeq.current;
    try {
      const { markdown } = await aiNoteComposeStream(
        {
          title,
          markdown: currentBody,
          instruction,
          model: config.model,
          cliPath: config.cliPath,
          provider: config.provider,
          cancelKey: key,
        },
        (delta) => {
          if (my !== runSeq.current) return; // 버려진 실행의 잔여 델타
          setStreamText((prev) => prev + delta);
        },
      );
      if (my !== runSeq.current) return; // 중단·재실행됨 — 이 결과로 화면을 덮지 않는다
      setResultMd(markdown);
      // 기존 노트 편집이면 변경점(diff)을 먼저 보여주고, 새 작성이면 미리보기
      setViewMode(hasExisting ? "diff" : "preview");
      setStep("preview");
    } catch (e) {
      // 사용자가 직접 끊은 실행은 에러가 아니다 — 죽은 CLI 의 비명을 노트에 띄우지 않는다
      if (my !== runSeq.current) return;
      setError(friendlyError(e));
      setStep("prompt");
    } finally {
      if (my === runSeq.current) cancelKey.current = null;
    }
  }

  /** 중단 — CLI 를 죽이고 **기다리지 않고** 바로 지시 화면으로 돌아간다.
   *  프로세스가 실제로 끝나는 걸 기다리면 누른 뒤에도 몇 초간 글자가 계속 흘러 안 먹은 듯 보인다. */
  function stop() {
    const k = cancelKey.current;
    runSeq.current++; // 이 시점 이후 도착하는 델타·결과·에러는 전부 무효
    cancelKey.current = null;
    if (k) void aiCancel(k);
    setStreamText("");
    setStep("prompt");
  }

  // 텍스트가 있는 프롬프트만 칩으로 (설정에서 추가만 하고 비워둔 것 제외)
  const savedUsable = saved.filter((p) => p.text.trim());

  const tooShort = instruction.trim().length < 2;

  // diff 안내 문구 — 언어별 어순이 달라 "{apply}" 자리에 <b>버튼 라벨</b>을 끼워 넣는다
  const diffHint = t("notes.ai.diffHint").split("{apply}");

  let footer: ReactNode = null;
  if (step === "prompt") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={run}
          disabled={tooShort || !config?.provider}
          title={!config ? t("notes.ai.configLoading") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("notes.ai.run")}
        </button>
      </>
    );
  } else if (step === "loading") {
    // 5분짜리 실행에 탈출구가 없으면 앱을 끄는 것 말고 방법이 없다
    footer = (
      <button className="btn btn-sm" onClick={stop}>
        <Icon name="x" size={14} />
        {t("notes.ai.stop")}
      </button>
    );
  } else if (step === "preview") {
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
            onApplied(resultMd);
            onClose();
          }}
        >
          <Icon name="check" size={15} />
          {hasExisting ? t("notes.ai.applyDiff") : t("notes.ai.applyNew")}
        </button>
      </>
    );
  }

  return (
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
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {savedUsable.map((p) => (
                  <span
                    key={p.id}
                    className="chip btn-like chip-saved"
                    title={p.text}
                    onClick={() => addPreset(p.text)}
                  >
                    <Icon name="sparkles" size={11} />
                    {p.label.trim() || p.text.slice(0, 20)}
                  </span>
                ))}
              </div>
            </div>
          )}
          <div className="field">
            <label>{t("notes.ai.presets")}</label>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {PRESETS.map((p) => (
                <span key={p} className="chip btn-like" onClick={() => addPreset(p)}>
                  + {p}
                </span>
              ))}
            </div>
          </div>
        </>
      )}

      {step === "loading" && (
        <div className="note-stream">
          <AiThinking
            compact={!!streamText}
            label={t("notes.ai.writing")}
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
              onChange={(e) => setResultMd(e.target.value)}
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
  );
}
