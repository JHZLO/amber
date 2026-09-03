import { useEffect, useState, type ReactNode } from "react";
import { Markdown } from "./Markdown";
import type { AppConfig } from "../lib/config";
import type { ConceptWithTags } from "../types";
import { aiAugment, friendlyError } from "../lib/ai";
import { setConceptTags, updateConceptContent } from "../lib/db";
import { writeNote } from "../lib/vault";
import { AiThinking, ChoiceChip, Modal } from "../ui";
import { composeInstruction } from "../lib/aiInstruction";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";

type Step = "prompt" | "loading" | "preview";

// 프롬프트 칸을 빠르게 채우는 자주 쓰는 보강 방향 — 화면에 보이는 칩이자 지시문 내용이라
// 현재 UI 언어를 따른다 (클릭 시 그 언어 그대로 AI 에 전달됨)
const PRESETS = [
  t("concepts.augment.preset.examples"),
  t("concepts.augment.preset.deeper"),
  t("concepts.augment.preset.simpler"),
  t("concepts.augment.preset.concise"),
  t("concepts.augment.preset.pitfalls"),
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
  // 체크한 빠른 지시(index) — 텍스트는 보낼 때 합친다
  const [chosen, setChosen] = useState<Set<number>>(() => new Set());
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
    setChosen(new Set());
    setError(null);
    setSaving(false);
    setTitle(concept.title);
    setSummary(concept.summary);
    setTags(concept.tags.join(", "));
    setBodyMd(currentBody);
    setShowSource(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function toggle(key: number) {
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  // 체크한 지시는 입력칸에 붙이지 않고 보낼 때 합친다 — 내가 친 말 → 빠른 지시
  const extras = PRESETS.filter((_, i) => chosen.has(i));
  const finalInstruction = composeInstruction(instruction, extras);

  async function run() {
    if (!config || finalInstruction.length < 2) return;
    setError(null);
    setStep("loading");
    try {
      const { note } = await aiAugment({
        title: concept.title,
        summary: concept.summary,
        tags: concept.tags,
        markdown: currentBody,
        instruction: finalInstruction,
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
      setError(t("concepts.form.required"));
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
      setError(errText(e));
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
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary"
          onClick={run}
          disabled={tooShort || !config?.provider}
          title={!config ? t("concepts.augment.loadingConfig") : undefined}
        >
          <Icon name="sparkles" size={15} />
          {t("concepts.augment.run")}
        </button>
      </>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("prompt")}>
          <Icon name="chevron-left" size={14} />
          {t("concepts.augment.again")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose} disabled={saving}>
          {t("common.cancel")}
        </button>
        <button className="btn btn-primary" onClick={apply} disabled={saving}>
          {saving ? t("concepts.augment.applying") : t("concepts.augment.apply")}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title={t("concepts.augment.title")} onClose={onClose} footer={footer} wide>
      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      {step === "prompt" && (
        <>
          <div className="field">
            <label>{t("concepts.augment.promptLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={4}
              placeholder={t("concepts.augment.promptPlaceholder")}
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">{t("concepts.augment.promptHint")}</div>
          </div>
          <div className="field">
            <label>{t("concepts.augment.presets")}</label>
            <div className="chip-row">
              {PRESETS.map((p, i) => (
                <ChoiceChip key={p} label={p} on={chosen.has(i)} onToggle={() => toggle(i)} />
              ))}
            </div>
            {extras.length > 0 && (
              <div className="hint">{t("common.ai.chosenCount", { n: extras.length })}</div>
            )}
          </div>
        </>
      )}

      {step === "loading" && (
        <AiThinking
          label={t("concepts.augment.thinking")}
          hint={t("concepts.augment.thinkingHint")}
        />
      )}

      {step === "preview" && (
        <>
          <div className="field">
            <label>{t("concepts.field.title")}</label>
            <input
              className="input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div className="field">
            <label>{t("concepts.field.summary")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={2}
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
            />
          </div>
          <div className="field">
            <label>{t("concepts.field.tags")}</label>
            <input
              className="input"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
            />
          </div>
          <div className="field">
            <label style={{ display: "flex", alignItems: "center" }}>
              {t("concepts.field.augmentedNote")}
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => setShowSource((v) => !v)}
              >
                {showSource ? t("concepts.preview.show") : t("concepts.preview.source")}
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
