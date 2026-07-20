import { useEffect, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import { ulid as genUlid } from "ulid";
import type { AppConfig } from "../lib/config";
import type { Confidence } from "../types";
import { aiGenerate, friendlyError } from "../lib/ai";
import { createConcept, getSetting, setSetting } from "../lib/db";
import { detailPathFor, writeNote } from "../lib/vault";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";

type Step = "paste" | "loading" | "preview";

function parseTags(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);
}

export function AddConceptModal({
  open,
  onClose,
  onCreated,
  config,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: () => void;
  config: AppConfig;
}) {
  const [step, setStep] = useState<Step>("paste");
  const [transcript, setTranscript] = useState("");
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [tags, setTags] = useState("");
  const [confidence, setConfidence] = useState<Confidence>(1);
  const [bodyMd, setBodyMd] = useState("");
  const [showPreview, setShowPreview] = useState(false);

  // 저장된 기본 지시문을 모달 열 때 불러옴 (한 번 써두면 계속 유지)
  useEffect(() => {
    if (open) getSetting("default_instruction").then((v) => v && setInstruction(v));
  }, [open]);

  function reset() {
    setStep("paste");
    setTranscript("");
    setError(null);
    setTitle("");
    setSummary("");
    setTags("");
    setConfidence(1);
    setBodyMd("");
    setShowPreview(false);
  }
  function close() {
    reset();
    onClose();
  }

  async function generate() {
    setError(null);
    setStep("loading");
    void setSetting("default_instruction", instruction.trim());
    try {
      const { note } = await aiGenerate({
        transcript,
        instruction,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
      });
      setTitle(note.title);
      setSummary(note.summary);
      setTags(note.tags.join(", "));
      setConfidence(note.confidence_suggestion);
      setBodyMd(note.detail_markdown);
      setStep("preview");
    } catch (e) {
      setError(friendlyError(e));
      setStep("paste");
    }
  }

  function manual() {
    setError(null);
    setTitle("");
    setSummary("");
    setTags("");
    setConfidence(1);
    setBodyMd("");
    setStep("preview");
  }

  async function save() {
    if (!title.trim() || !summary.trim()) {
      setError("제목과 요약은 필수예요.");
      return;
    }
    setSaving(true);
    try {
      const id = genUlid();
      await writeNote(id, bodyMd);
      await createConcept({
        ulid: id,
        title: title.trim(),
        summary: summary.trim(),
        detailPath: detailPathFor(id),
        tags: parseTags(tags),
        confidence,
        source: transcript || null,
        sourceKind: "paste",
      });
      onCreated();
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const tooShort = transcript.trim().length < 20;

  let footer: ReactNode = null;
  if (step === "paste") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={manual}>
          수동 작성
        </button>
        <button
          className="btn btn-primary"
          onClick={generate}
          disabled={tooShort}
        >
          <Icon name="sparkles" size={15} />
          Claude로 정리
        </button>
      </>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("paste")}>
          <Icon name="chevron-left" size={14} />
          원문으로
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={close}>
          취소
        </button>
        <button className="btn btn-primary" onClick={save} disabled={saving}>
          {saving ? "저장 중…" : "저장 (학습중으로)"}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title="새 개념 추가" onClose={close} footer={footer} wide>
      {error && <div className="error-note" style={{ marginBottom: 12 }}>{error}</div>}

      {step === "paste" && (
        <>
          <div className="field">
            <label>AI와 나눈 Q&A 원문을 붙여넣으세요</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={12}
              placeholder="여기에 대화 전체를 붙여넣기…"
              value={transcript}
              onChange={(e) => setTranscript(e.target.value)}
            />
          </div>
          <div className="hint">
            {transcript.length}자 {tooShort && "· 최소 20자 이상 입력하세요"}
          </div>
          <div className="field" style={{ marginTop: 16 }}>
            <label>지시문 (선택) — Claude에게 정리 방향 지시</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={3}
              placeholder="예: Postgres 관점 위주로 · 초보자도 이해하게 · 코드 예시 꼭 포함…"
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">저장돼서 다음 추가 때도 기본값으로 채워져요.</div>
          </div>
        </>
      )}

      {step === "loading" && (
        <AiThinking
          label="Claude가 정리하는 중…"
          hint="원문에서 핵심 개념 → 요약 → 상세 노트"
        />
      )}

      {step === "preview" && (
        <>
          <div className="field">
            <label>제목</label>
            <input
              className="input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div className="field">
            <label>요약 (위젯 표시용)</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={2}
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
            />
          </div>
          <div className="field">
            <label>태그 (쉼표로 구분)</label>
            <input
              className="input"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
            />
          </div>
          <div className="field">
            <label>자신감</label>
            <div style={{ display: "flex", gap: 6 }}>
              {[1, 2, 3].map((n) => (
                <button
                  key={n}
                  className={`btn btn-sm ${confidence === n ? "btn-primary" : ""}`}
                  onClick={() => setConfidence(n as Confidence)}
                >
                  {n}
                </button>
              ))}
              <span className="hint" style={{ alignSelf: "center", marginLeft: 6 }}>
                방금 배운 것은 1을 추천
              </span>
            </div>
          </div>
          <div className="field">
            <label style={{ display: "flex", alignItems: "center" }}>
              상세 노트 (Markdown)
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => setShowPreview((v) => !v)}
              >
                {showPreview ? "소스 편집" : "프리뷰"}
              </button>
            </label>
            {showPreview ? (
              <div className="markdown md-preview">
                <Markdown>{bodyMd}</Markdown>
              </div>
            ) : (
              <textarea
                className="textarea"
                rows={16}
                value={bodyMd}
                onChange={(e) => setBodyMd(e.target.value)}
              />
            )}
          </div>
        </>
      )}
    </Modal>
  );
}
