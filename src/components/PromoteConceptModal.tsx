// 필기노트의 선택 영역을 개념 카드로 "승격".
// 선택 텍스트 + 노트 문맥을 개념 생성 파이프라인(aiGenerate)에 넣어 제목/요약/상세/태그를 만들고,
// 미리보기에서 편집 후 저장한다. 저장 = 개념 생성(source_kind='file') + 노트 쪽 역참조 사이드카 기록.

import { useEffect, useState, type ReactNode } from "react";
import { ulid as genUlid } from "ulid";
import { Markdown } from "./Markdown";
import type { AppConfig } from "../lib/config";
import type { Confidence } from "../types";
import { aiGenerate, friendlyError } from "../lib/ai";
import { createConcept } from "../lib/db";
import { detailPathFor, writeNote } from "../lib/vault";
import { addNoteConcept } from "../lib/noteConcepts";
import { AiThinking, Modal } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";

type Step = "loading" | "preview";

function parseTags(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);
}

export interface PromoteTarget {
  noteRel: string;
  /** 드래그한 선택 텍스트 */
  selection: string;
  /** 노트 전체(문맥용) */
  noteBody: string;
}

export function PromoteConceptModal({
  target,
  config,
  onClose,
  onDone,
}: {
  target: PromoteTarget | null;
  config: AppConfig | null;
  onClose: () => void;
  onDone: (conceptId: number, title: string) => void;
}) {
  const open = !!target;
  const [step, setStep] = useState<Step>("loading");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [showSource, setShowSource] = useState(false);

  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [tags, setTags] = useState("");
  const [confidence, setConfidence] = useState<Confidence>(1);
  const [bodyMd, setBodyMd] = useState("");

  // 열릴 때마다 선택 기준으로 새로 생성
  useEffect(() => {
    if (!target) return;
    let alive = true;
    setStep("loading");
    setError(null);
    setSaving(false);
    setShowSource(false);

    if (!config?.provider) {
      // AI 미연결: 선택을 시드로 수동 편집
      setTitle(target.selection.slice(0, 40));
      setSummary("");
      setTags("");
      setConfidence(1);
      setBodyMd(target.selection);
      setStep("preview");
      return;
    }

    const input =
      `[개념으로 만들 부분]\n${target.selection}\n\n` +
      `[출처 노트 전체 (문맥)]\n${target.noteBody}`;
    const instruction =
      "위 [개념으로 만들 부분]을 복습용 개념 카드로 만들어라. 그 부분을 중심 주제로 삼되, " +
      "[출처 노트 전체]를 문맥으로만 참고해 제목·요약·상세 노트·태그를 작성하라. " +
      "노트 전체를 요약하지 말고 선택한 개념에 집중하라.";

    aiGenerate({
      transcript: input,
      instruction,
      model: config.model,
      cliPath: config.cliPath,
      provider: config.provider,
    })
      .then(({ note }) => {
        if (!alive) return;
        setTitle(note.title);
        setSummary(note.summary);
        setTags(note.tags.join(", "));
        setConfidence(note.confidence_suggestion);
        setBodyMd(note.detail_markdown);
        setStep("preview");
      })
      .catch((e) => {
        if (!alive) return;
        // 실패해도 선택 텍스트로 수동 진행 가능하게
        setError(friendlyError(e));
        setTitle(target.selection.slice(0, 40));
        setBodyMd(target.selection);
        setStep("preview");
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  async function save() {
    if (!target) return;
    if (!title.trim() || !summary.trim()) {
      setError(t("concepts.form.required"));
      return;
    }
    setSaving(true);
    try {
      const id = genUlid();
      await writeNote(id, bodyMd);
      const conceptId = await createConcept({
        ulid: id,
        title: title.trim(),
        summary: summary.trim(),
        detailPath: detailPathFor(id),
        tags: parseTags(tags),
        confidence,
        // 출처 = 노트경로 + 앵커(선택 텍스트). 개념→노트 이동에 사용
        source: JSON.stringify({ noteRel: target.noteRel, anchor: target.selection }),
        sourceKind: "file",
      });
      // 노트 쪽 역참조 기록 (노트→개념)
      await addNoteConcept(target.noteRel, {
        conceptId,
        title: title.trim(),
        anchor: target.selection,
        createdAt: Date.now(),
      });
      onDone(conceptId, title.trim());
    } catch (e) {
      setError(errText(e));
    } finally {
      setSaving(false);
    }
  }

  let footer: ReactNode = null;
  if (step === "preview") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={onClose} disabled={saving}>
          {t("common.cancel")}
        </button>
        <button className="btn btn-primary" onClick={() => void save()} disabled={saving}>
          <Icon name="layers" size={15} />
          {saving ? t("concepts.saving") : t("concepts.promote.save")}
        </button>
      </>
    );
  }

  return (
    <Modal open={open} title={t("concepts.promote.title")} onClose={onClose} footer={footer} wide>
      {step === "loading" && (
        <AiThinking
          label={t("concepts.promote.thinking")}
          hint={t("concepts.promote.thinkingHint")}
        />
      )}

      {step === "preview" && (
        <>
          {error && (
            <div className="error-note" style={{ marginBottom: 12 }}>
              {error}
            </div>
          )}
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
              {t("concepts.field.detailNote")}
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
                rows={14}
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
