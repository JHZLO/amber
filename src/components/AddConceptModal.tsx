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
import { t } from "../lib/i18n";

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
      setError(t("concepts.form.required"));
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
          {t("concepts.add.manual")}
        </button>
        <button
          className="btn btn-primary"
          onClick={generate}
          disabled={tooShort}
        >
          <Icon name="sparkles" size={15} />
          {t("concepts.add.generate")}
        </button>
      </>
    );
  } else if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("paste")}>
          <Icon name="chevron-left" size={14} />
          {t("concepts.add.backToSource")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={close}>
          {t("common.cancel")}
        </button>
        <button className="btn btn-primary" onClick={save} disabled={saving}>
          {saving ? t("concepts.saving") : t("concepts.add.save")}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title={t("concepts.add.title")} onClose={close} footer={footer} wide>
      {error && <div className="error-note" style={{ marginBottom: 12 }}>{error}</div>}

      {step === "paste" && (
        <>
          <div className="field">
            <label>{t("concepts.add.pasteLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={12}
              placeholder={t("concepts.add.pastePlaceholder")}
              value={transcript}
              onChange={(e) => setTranscript(e.target.value)}
            />
          </div>
          <div className="hint">
            {t("concepts.add.charCount", { n: transcript.length })}{" "}
            {tooShort && t("concepts.add.tooShort")}
          </div>
          <div className="field" style={{ marginTop: 16 }}>
            <label>{t("concepts.add.instructionLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={3}
              placeholder={t("concepts.add.instructionPlaceholder")}
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
            />
            <div className="hint">{t("concepts.add.instructionHint")}</div>
          </div>
        </>
      )}

      {step === "loading" && (
        <AiThinking
          label={t("concepts.add.thinking")}
          hint={t("concepts.add.thinkingHint")}
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
            <label>{t("concepts.field.confidence")}</label>
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
                {t("concepts.add.confidenceHint")}
              </span>
            </div>
          </div>
          <div className="field">
            <label style={{ display: "flex", alignItems: "center" }}>
              {t("concepts.field.detailNote")}
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => setShowPreview((v) => !v)}
              >
                {showPreview ? t("concepts.preview.source") : t("concepts.preview.show")}
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
