// 필기노트 AI 작성 모달: 지시 → claude_note_compose → 프리뷰 → 에디터 초안으로 적용.
// 파일에 바로 저장하지 않는다 — 적용 후 사용자가 라이브 프리뷰로 확인하고 ⌘S 로 저장 (AI 출력은 초안).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import { DiffView } from "./DiffView";
import type { AppConfig } from "../lib/config";
import { claudeNoteComposeStream, friendlyError } from "../lib/claude";
import { loadPrompts, type SavedPrompt } from "../lib/prompts";
import { Modal, Spinner } from "../ui";
import { Icon } from "../icons";

type Step = "prompt" | "loading" | "preview";
type ViewMode = "diff" | "preview" | "source";

// 자주 쓰는 작성 방향 (빈 노트 = 처음부터, 채워진 노트 = 보강)
const PRESETS = [
  "이 주제로 처음부터 정리",
  "구체적인 예시·코드 추가",
  "더 깊고 자세하게",
  "핵심만 간결하게 압축",
  "표로 정리",
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

  // 편집(기존 내용 있음) vs 새로 작성 구분 — diff 는 기존 내용이 있을 때만 의미
  const hasExisting = currentBody.trim().length > 0;

  // 열 때마다 초기화 (닫혀 있는 동안의 stale 상태 방지) + 저장 프롬프트 최신 로드
  useEffect(() => {
    if (!open) return;
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
    try {
      const { markdown } = await claudeNoteComposeStream(
        {
          title,
          markdown: currentBody,
          instruction,
          model: config.model,
          claudePath: config.claudePath,
        },
        (delta) => setStreamText((prev) => prev + delta),
      );
      setResultMd(markdown);
      // 기존 노트 편집이면 변경점(diff)을 먼저 보여주고, 새 작성이면 미리보기
      setViewMode(hasExisting ? "diff" : "preview");
      setStep("preview");
    } catch (e) {
      setError(friendlyError(e));
      setStep("prompt");
    }
  }

  // 텍스트가 있는 프롬프트만 칩으로 (설정에서 추가만 하고 비워둔 것 제외)
  const savedUsable = saved.filter((p) => p.text.trim());

  const tooShort = instruction.trim().length < 2;

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
          disabled={tooShort || !config}
          title={!config ? "설정을 불러오는 중이에요" : undefined}
        >
          <Icon name="sparkles" size={15} />
          AI로 작성
        </button>
      </>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("prompt")}>
          <Icon name="chevron-left" size={14} />
          다시 지시
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose}>
          취소
        </button>
        <button
          className="btn btn-primary"
          onClick={() => {
            onApplied(resultMd);
            onClose();
          }}
        >
          <Icon name="check" size={15} />
          {hasExisting ? "변경 적용" : "에디터에 적용"}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title="AI로 노트 작성" onClose={onClose} footer={footer} wide>
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "prompt" && (
        <>
          <div className="field">
            <label>무엇을 써 드릴까요? — Claude에게 지시</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={4}
              placeholder="예: Rust 변수와 가변성(mut, shadowing)을 예제 코드와 함께 정리해줘 · 지금 노트에 소유권과의 관계 섹션을 추가해줘…"
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">
              현재 노트가 비어 있으면 처음부터 작성하고, 내용이 있으면 문체·구조를
              보존하며 보강해요. 결과는 에디터 초안으로 들어가니 확인 후 ⌘S로
              저장하세요.
            </div>
          </div>
          {savedUsable.length > 0 && (
            <div className="field">
              <label>내 프롬프트</label>
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
            <label>빠른 지시</label>
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
          <div className="note-stream-head">
            <Spinner />
            <span>Claude가 작성하는 중…</span>
          </div>
          {streamText ? (
            <pre className="note-stream-body" ref={streamRef}>
              {streamText}
              <span className="stream-caret" />
            </pre>
          ) : (
            <div className="hint">응답을 기다리는 중…</div>
          )}
        </div>
      )}

      {step === "preview" && (
        <div className="field">
          <label style={{ display: "flex", alignItems: "center" }}>
            {hasExisting ? "AI 편집 결과" : "작성 결과"}
            <span className="spacer" />
            <div className="segmented">
              {hasExisting && (
                <button
                  className={`tab ${viewMode === "diff" ? "active" : ""}`}
                  onClick={() => setViewMode("diff")}
                >
                  변경사항
                </button>
              )}
              <button
                className={`tab ${viewMode === "preview" ? "active" : ""}`}
                onClick={() => setViewMode("preview")}
              >
                미리보기
              </button>
              <button
                className={`tab ${viewMode === "source" ? "active" : ""}`}
                onClick={() => setViewMode("source")}
              >
                소스
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
              현재 노트와 비교한 변경점이에요. <b>변경 적용</b>을 누르면 결과가
              에디터 초안으로 들어가고, 저장(⌘S) 전까지 파일은 그대로예요.
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
