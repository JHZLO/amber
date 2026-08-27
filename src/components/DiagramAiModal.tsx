// 다이어그램 AI 모달: 스키마 DDL 붙여넣기 → ai_erd_generate_stream → 프리뷰 → 에디터 초안으로 적용.
// 파일에 바로 저장하지 않는다 — 적용 후 사용자가 라이브 프리뷰로 확인하고 ⌘S 로 저장 (AI 출력은 초안).
// 변환 규칙(선 종류·타입 뒤 ? 로 nullable·(enc)·인덱스 표기 등)은 src-tauri/context/diagram-erd.md 가 정본.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Mermaid } from "./Mermaid";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { aiCancel, aiErdGenerateStream, friendlyError, newCancelKey } from "../lib/ai";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

type Step = "prompt" | "loading" | "preview";
type ViewMode = "diff" | "preview" | "source";

// 변환 방향을 바꾸는 자주 쓰는 단서들 (프롬프트의 기본 규칙을 덮어쓴다)
const PRESETS = [
  t("diagrams.ai.preset.noAudit"),
  t("diagrams.ai.preset.noIndex"),
  t("diagrams.ai.preset.inferFk"),
  t("diagrams.ai.preset.coreOnly"),
  t("diagrams.ai.preset.append"),
];

export function DiagramAiModal({
  open,
  currentSource,
  config,
  onClose,
  onApplied,
}: {
  open: boolean;
  currentSource: string;
  config: AppConfig | null;
  onClose: () => void;
  onApplied: (mermaid: string) => void;
}) {
  const [step, setStep] = useState<Step>("prompt");
  const [ddl, setDdl] = useState("");
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState("");
  const [streamText, setStreamText] = useState(""); // 생성 중 실시간 누적 텍스트
  const [viewMode, setViewMode] = useState<ViewMode>("preview");
  const streamRef = useRef<HTMLPreElement>(null);
  // 진행 중인 실행의 취소 키 — 중단 버튼이 이걸로 CLI 를 끝낸다
  const cancelKey = useRef<string | null>(null);
  // 실행 세대 — 버려진 실행의 델타가 새 버퍼에 섞이지 않게 (NoteAiModal 과 같은 이유)
  const runSeq = useRef(0);

  // 빈 다이어그램이면 비교할 대상이 없으니 diff 탭을 숨긴다
  const hasExisting = currentSource.trim().length > 0;

  // 열 때마다 초기화 (닫혀 있는 동안의 stale 상태 방지)
  useEffect(() => {
    if (!open) return;
    runSeq.current++; // 닫힌 동안 계속 돌던 실행의 델타를 이 세션에서 끊는다
    setStep("prompt");
    setDdl("");
    setInstruction("");
    setError(null);
    setResult("");
    setStreamText("");
    setViewMode("preview");
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
    if (!config || ddl.trim().length < 20) return;
    setError(null);
    setStreamText("");
    setStep("loading");
    const key = newCancelKey();
    cancelKey.current = key;
    const my = ++runSeq.current;
    try {
      const { mermaid } = await aiErdGenerateStream(
        {
          ddl,
          instruction,
          current: currentSource,
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
      setResult(mermaid);
      setViewMode("preview");
      setStep("preview");
    } catch (e) {
      if (my !== runSeq.current) return; // 사용자가 직접 끊은 실행은 에러가 아니다
      setError(friendlyError(e));
      setStep("prompt");
    } finally {
      if (my === runSeq.current) cancelKey.current = null;
    }
  }

  /** 중단 — CLI 를 죽이고 기다리지 않고 바로 지시 화면으로 (NoteAiModal.stop 과 같은 규약) */
  function stop() {
    const k = cancelKey.current;
    runSeq.current++;
    cancelKey.current = null;
    if (k) void aiCancel(k);
    setStreamText("");
    setStep("prompt");
  }

  const tooShort = ddl.trim().length < 20;

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
          title={!config ? t("diagrams.ai.configLoading") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("diagrams.ai.convert")}
        </button>
      </>
    );
  } else if (step === "loading") {
    footer = (
      <button className="btn btn-sm btn-danger-ghost" onClick={stop}>
        <Icon name="x" size={14} />
        {t("diagrams.ai.stop")}
      </button>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("prompt")}>
          <Icon name="chevron-left" size={14} />
          {t("diagrams.ai.back")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={() => {
            onApplied(result);
            onClose();
          }}
        >
          <Icon name="check" size={15} />
          {t("diagrams.ai.apply")}
        </button>
      </>
    );
  }

  return (
    <Modal
      open={open}
      title={t("diagrams.ai.title")}
      onClose={onClose}
      footer={footer}
      wide
      fixedHeight
    >
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "prompt" && (
        <>
          <div className="field">
            <label>{t("diagrams.ai.ddlLabel")}</label>
            <textarea
              className="textarea"
              rows={12}
              spellCheck={false}
              placeholder={t("diagrams.ai.ddlPh")}
              value={ddl}
              onChange={(e) => setDdl(e.target.value)}
            />
            <div className="hint">
              {t("diagrams.ai.ddlHint1")}
              <code>varchar?</code>
              {t("diagrams.ai.ddlHint2")}
              <code>(enc)</code>
              {t("diagrams.ai.ddlHint3")}
            </div>
          </div>
          <div className="field">
            <label>{t("diagrams.ai.instrLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={2}
              placeholder={t("diagrams.ai.instrPh")}
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
          </div>
          <div className="field">
            <label>{t("diagrams.ai.presetsLabel")}</label>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              {PRESETS.map((p) => (
                <span
                  key={p}
                  className="chip btn-like"
                  onClick={() => addPreset(p)}
                >
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
            label={t("diagrams.ai.generating")}
            hint={streamText ? undefined : t("diagrams.ai.waiting")}
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
            {t("diagrams.ai.resultLabel")}
            <span className="spacer" />
            <div className="segmented">
              <button
                className={`tab ${viewMode === "preview" ? "active" : ""}`}
                onClick={() => setViewMode("preview")}
              >
                {t("diagrams.ai.tabDiagram")}
              </button>
              <button
                className={`tab ${viewMode === "source" ? "active" : ""}`}
                onClick={() => setViewMode("source")}
              >
                {t("diagrams.ai.tabSource")}
              </button>
              {hasExisting && (
                <button
                  className={`tab ${viewMode === "diff" ? "active" : ""}`}
                  onClick={() => setViewMode("diff")}
                >
                  {t("diagrams.ai.tabDiff")}
                </button>
              )}
            </div>
          </label>
          {viewMode === "source" ? (
            <textarea
              className="textarea"
              rows={18}
              spellCheck={false}
              value={result}
              onChange={(e) => setResult(e.target.value)}
            />
          ) : viewMode === "diff" ? (
            <DiffView oldText={currentSource} newText={result} />
          ) : (
            <Mermaid chart={result} />
          )}
          <div className="hint">
            <b>{t("diagrams.ai.apply")}</b>
            {t("diagrams.ai.applyHint")}
          </div>
        </div>
      )}
    </Modal>
  );
}
