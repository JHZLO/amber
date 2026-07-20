import { useEffect, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import type { AppConfig } from "../lib/config";
import type { ConceptWithTags } from "../types";
import { aiAugment, friendlyError } from "../lib/ai";
import { setConceptTags, updateConceptContent } from "../lib/db";
import { writeNote } from "../lib/vault";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";

type Step = "prompt" | "loading" | "preview";

// 프롬프트 칸을 빠르게 채우는 자주 쓰는 보강 방향
const PRESETS = [
  "구체적인 예시·코드 추가",
  "더 깊고 자세하게",
  "초보자도 이해하게 쉽게",
  "핵심만 간결하게 압축",
  "주의점·함정 보강",
];

function parseTags(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);
}

export function AugmentModal({
  open,
  concept,
  currentBody,
  config,
  onClose,
  onApplied,
}: {
  open: boolean;
  concept: ConceptWithTags;
  currentBody: string;
  config: AppConfig | null;
  onClose: () => void;
  onApplied: (newBody: string) => void;
}) {
  const [step, setStep] = useState<Step>("prompt");
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // 보강 결과 (프리뷰에서 사용자가 더 손볼 수 있게 편집 가능하게 둔다)
  const [title, setTitle] = useState(concept.title);
  const [summary, setSummary] = useState(concept.summary);
  const [tags, setTags] = useState(concept.tags.join(", "));
  const [bodyMd, setBodyMd] = useState(currentBody);
  const [showSource, setShowSource] = useState(false);

  // 열 때마다 현재 노트 기준으로 새로 시작 (닫혀 있는 동안의 stale 상태 방지)
  useEffect(() => {
    if (!open) return;
    setStep("prompt");
    setInstruction("");
    setError(null);
    setSaving(false);
    setTitle(concept.title);
    setSummary(concept.summary);
    setTags(concept.tags.join(", "));
    setBodyMd(currentBody);
    setShowSource(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function addPreset(p: string) {
    setInstruction((prev) => (prev.trim() ? `${prev.trim()} · ${p}` : p));
  }

  async function run() {
    if (!config || instruction.trim().length < 2) return;
    setError(null);
    setStep("loading");
    try {
      const { note } = await aiAugment({
        title: concept.title,
        summary: concept.summary,
        tags: concept.tags,
        markdown: currentBody,
        instruction,
        model: config.model,
        cliPath: config.cliPath,
        provider: config.provider,
      });
      setTitle(note.title);
      setSummary(note.summary);
      setTags(note.tags.join(", "));
      setBodyMd(note.detail_markdown);
      setShowSource(false);
      setStep("preview");
    } catch (e) {
      setError(friendlyError(e));
      setStep("prompt");
    }
  }

  async function apply() {
    if (!title.trim() || !summary.trim()) {
      setError("제목과 요약은 필수예요.");
      return;
    }
    setSaving(true);
    try {
      // 자신감/학습상태는 보강이 건드리지 않는다 (내용만 갱신)
      await updateConceptContent(concept.id, {
        title: title.trim(),
        summary: summary.trim(),
      });
      await setConceptTags(concept.id, parseTags(tags));
      await writeNote(concept.ulid, bodyMd);
      onApplied(bodyMd);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

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
          disabled={tooShort || !config?.provider}
          title={!config ? "설정을 불러오는 중이에요" : undefined}
        >
          <Icon name="sparkles" size={15} />
          AI로 보강
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
        <button className="btn btn-sm" onClick={onClose} disabled={saving}>
          취소
        </button>
        <button className="btn btn-primary" onClick={apply} disabled={saving}>
          {saving ? "적용 중…" : "적용"}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title="AI로 노트 보강" onClose={onClose} footer={footer} wide>
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "prompt" && (
        <>
          <div className="field">
            <label>어떻게 보강할까요? — Claude에게 지시</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={4}
              placeholder="예: kube-proxy IPVS 모드 설정 예시를 코드블록으로 추가해줘 · 성능 비교 부분을 더 깊게 · 표로 정리해줘…"
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">
              현재 노트 전체를 바탕으로 다시 정리해요. 자신감·학습상태는 그대로 유지돼요.
            </div>
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
        <AiThinking
          label="Claude가 노트를 보강하는 중…"
          hint="현재 노트 + 지시 → 보강된 상세 노트"
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
            <label style={{ display: "flex", alignItems: "center" }}>
              보강된 상세 노트 (Markdown)
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => setShowSource((v) => !v)}
              >
                {showSource ? "프리뷰" : "소스 편집"}
              </button>
            </label>
            {showSource ? (
              <textarea
                className="textarea"
                rows={16}
                value={bodyMd}
                onChange={(e) => setBodyMd(e.target.value)}
              />
            ) : (
              <div className="markdown md-preview">
                <Markdown>{bodyMd}</Markdown>
              </div>
            )}
          </div>
        </>
      )}
    </Modal>
  );
}
