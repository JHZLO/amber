// 다이어그램 AI 모달: 스키마 DDL 붙여넣기 → ai_erd_generate_stream → 프리뷰 → 에디터 초안으로 적용.
// 파일에 바로 저장하지 않는다 — 적용 후 사용자가 라이브 프리뷰로 확인하고 ⌘S 로 저장 (AI 출력은 초안).
// 변환 규칙(선 종류·[NOTNULL] 태그·(enc)·인덱스 표기 등)은 src-tauri/context/diagram-erd.md 가 정본.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Mermaid } from "./Mermaid";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { aiErdGenerateStream, friendlyError } from "../lib/ai";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";

type Step = "prompt" | "loading" | "preview";
type ViewMode = "diff" | "preview" | "source";

// 변환 방향을 바꾸는 자주 쓰는 단서들 (프롬프트의 기본 규칙을 덮어쓴다)
const PRESETS = [
  "감사(_aud) 테이블 제외",
  "인덱스 표기 생략",
  "논리 FK 도 추론해서 연결",
  "핵심 테이블만 추리기",
  "현재 다이어그램에 이어 붙이기",
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

  // 빈 다이어그램이면 비교할 대상이 없으니 diff 탭을 숨긴다
  const hasExisting = currentSource.trim().length > 0;

  // 열 때마다 초기화 (닫혀 있는 동안의 stale 상태 방지)
  useEffect(() => {
    if (!open) return;
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
    try {
      const { mermaid } = await aiErdGenerateStream(
        {
          ddl,
          instruction,
          current: currentSource,
          model: config.model,
          cliPath: config.cliPath,
          provider: config.provider,
        },
        (delta) => setStreamText((prev) => prev + delta),
      );
      setResult(mermaid);
      setViewMode("preview");
      setStep("preview");
    } catch (e) {
      setError(friendlyError(e));
      setStep("prompt");
    }
  }

  const tooShort = ddl.trim().length < 20;

  let footer: ReactNode = null;
  if (step === "prompt") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={onClose}>
          취소
        </button>
        <button
          className="btn btn-primary"
          onClick={run}
          disabled={tooShort || !config?.provider}
          title={!config ? "설정을 불러오는 중이에요" : undefined}
        >
          <Icon name="sparkles" size={15} />
          ERD로 변환
        </button>
      </>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("prompt")}>
          <Icon name="chevron-left" size={14} />
          다시 변환
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          취소
        </button>
        <button
          className="btn btn-primary"
          onClick={() => {
            onApplied(result);
            onClose();
          }}
        >
          <Icon name="check" size={15} />
          에디터에 적용
        </button>
      </>
    );
  }

  return (
    <Modal
      open={open}
      title="DDL → ERD 변환"
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
            <label>스키마 DDL</label>
            <textarea
              className="textarea"
              rows={12}
              spellCheck={false}
              placeholder={"CREATE TABLE ts_order (\n  id BIGINT NOT NULL AUTO_INCREMENT COMMENT '주문 ID',\n  ...\n);"}
              value={ddl}
              onChange={(e) => setDdl(e.target.value)}
            />
            <div className="hint">
              CREATE TABLE · ALTER TABLE 를 그대로 붙여넣으세요. 컬럼 COMMENT ·
              인덱스 · UNIQUE · FK 제약을 읽어 ERD 표기 규칙(실선=물리 FK,
              점선=논리 참조, <code>[NOTNULL]</code> 태그, <code>(enc)</code>,
              enum 나열)에 맞춰 변환해요.
            </div>
          </div>
          <div className="field">
            <label>추가 지시 (선택)</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={2}
              placeholder="예: 결제 관련 테이블만 · 컬럼 설명은 짧게 · 감사 테이블은 빼줘"
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
          </div>
          <div className="field">
            <label>빠른 지시</label>
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
            label="스키마를 ERD로 옮기는 중…"
            hint={streamText ? undefined : "응답을 기다리는 중…"}
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
            변환 결과
            <span className="spacer" />
            <div className="segmented">
              <button
                className={`tab ${viewMode === "preview" ? "active" : ""}`}
                onClick={() => setViewMode("preview")}
              >
                다이어그램
              </button>
              <button
                className={`tab ${viewMode === "source" ? "active" : ""}`}
                onClick={() => setViewMode("source")}
              >
                소스
              </button>
              {hasExisting && (
                <button
                  className={`tab ${viewMode === "diff" ? "active" : ""}`}
                  onClick={() => setViewMode("diff")}
                >
                  변경사항
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
            <b>에디터에 적용</b>을 누르면 결과가 초안으로 들어가고, 저장(⌘S)
            전까지 파일은 그대로예요.
          </div>
        </div>
      )}
    </Modal>
  );
}
