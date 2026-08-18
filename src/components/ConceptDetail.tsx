import { useEffect, useState } from "react";
import { Markdown } from "./Markdown";
import type { AppConfig } from "../lib/config";
import type { ConceptWithTags } from "../types";
import {
  adjustConfidence,
  deleteConcept,
  setConceptTags,
  setStatus,
  updateConceptContent,
} from "../lib/db";
import { deleteConceptDir, noteExists, readNote, writeNote } from "../lib/vault";
import { removeNoteConcept } from "../lib/noteConcepts";
import { ConfidenceDots, Modal, Spinner, StatusBadge, Tooltip, timeAgo } from "../ui";
import { getRoot, rootDisplayName } from "../lib/workspace";
import { Icon } from "../icons";
import { AugmentModal } from "./AugmentModal";
import { openNoteInApp } from "../lib/nav";
import { dateLocale, t } from "../lib/i18n";
import { errText } from "../lib/errors";

const errMsg = errText; // Rust 코드화 에러까지 번역 (lib/errors.ts)

function parseTags(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);
}

/** 필기노트 승격으로 만든 개념이면 source(JSON)에서 출처 노트를 파싱.
 *  root 는 v0.17.11 부터 기록 — 없는 구형 행은 null 이고, 그 경우 현재 루트를 그대로 믿는다(기존 동작). */
function parseSourceNote(
  concept: ConceptWithTags,
): { root: string | null; noteRel: string; anchor: string } | null {
  if (concept.source_kind !== "file" || !concept.source) return null;
  try {
    const d = JSON.parse(concept.source);
    if (d && typeof d.noteRel === "string") {
      return {
        root: typeof d.root === "string" ? d.root : null,
        noteRel: d.noteRel,
        anchor: String(d.anchor ?? ""),
      };
    }
  } catch {
    /* 구형/비JSON source 는 무시 */
  }
  return null;
}

export function ConceptDetail({
  concept,
  config,
  onChanged,
  onDirtyChange,
}: {
  concept: ConceptWithTags;
  config: AppConfig | null;
  onChanged: (opts?: { deleted?: boolean }) => void;
  /** 미저장 초안 여부를 상위로 — App 이 선택/필터 변경을 가로채 확인 모달을 띄운다 */
  onDirtyChange?: (dirty: boolean) => void;
}) {
  const [body, setBody] = useState("");
  const [loadingBody, setLoadingBody] = useState(true);
  const [readError, setReadError] = useState<string | null>(null);
  // 파일만 사라진 경우 — 읽기 실패지만 편집/저장은 열어 둬야 복구가 된다
  const [missingFile, setMissingFile] = useState(false);
  const [writeError, setWriteError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [augmenting, setAugmenting] = useState(false);

  const sourceNote = parseSourceNote(concept);
  // 기록된 루트가 지금 열려 있는 노트 루트와 다르면 같은 이름의 다른 파일을 열게 된다 — 막는다
  const otherRoot = !!sourceNote?.root && sourceNote.root !== getRoot("notes");

  // 삭제 확인문 — 제목(<b>)이 문장 어디에 오는지 언어마다 다르므로 {title} 자리에서 쪼갠다
  const delConfirm = t("concepts.delete.confirm").split("{title}");

  const [dTitle, setDTitle] = useState(concept.title);
  const [dSummary, setDSummary] = useState(concept.summary);
  const [dTags, setDTags] = useState(concept.tags.join(", "));
  const [dBody, setDBody] = useState("");

  useEffect(() => {
    let alive = true;
    setEditing(false);
    setReadError(null);
    setWriteError(null);
    setMissingFile(false);
    setLoadingBody(true);
    readNote(concept.detail_path)
      .then((b) => alive && setBody(b))
      // 실패를 본문으로 위장하지 않는다 — 그 문자열이 초안이 되면 저장 시 원본이 날아간다.
      // 단 '파일이 아예 없음'은 읽기 오류가 아니라 복구 대상이다 — 편집을 막으면 영구 읽기전용이 된다.
      .catch(async (e) => {
        if (!alive) return;
        setBody("");
        const gone = !(await noteExists(concept.detail_path));
        if (!alive) return;
        if (gone) setMissingFile(true);
        else setReadError(errMsg(e));
      })
      .finally(() => alive && setLoadingBody(false));
    return () => {
      alive = false;
    };
  }, [concept.id, concept.detail_path]);

  const dirty =
    editing &&
    (dTitle !== concept.title ||
      dSummary !== concept.summary ||
      dTags !== concept.tags.join(", ") ||
      dBody !== body);

  // 언마운트 시에도 반드시 false 로 되돌린다 — 안 그러면 App 이 영원히 초안이 있다고 믿는다
  useEffect(() => {
    onDirtyChange?.(dirty);
    return () => onDirtyChange?.(false);
  }, [dirty, onDirtyChange]);

  function startEdit() {
    setDTitle(concept.title);
    setDSummary(concept.summary);
    setDTags(concept.tags.join(", "));
    setDBody(body);
    setEditing(true);
  }

  async function save() {
    if (busy || readError) return;
    setBusy(true);
    setWriteError(null);
    try {
      // 파일을 먼저 — 실패해도 DB 가 파일보다 앞서 나가지 않게 (AugmentModal.apply 와 같은 순서 규칙)
      await writeNote(concept.ulid, dBody);
      await updateConceptContent(concept.id, {
        title: dTitle.trim(),
        summary: dSummary.trim(),
      });
      await setConceptTags(concept.id, parseTags(dTags));
      setBody(dBody);
      setMissingFile(false);
      setEditing(false);
      onChanged();
    } catch (e) {
      setWriteError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  async function changeConfidence(delta: number) {
    if (busy) return;
    setBusy(true);
    setWriteError(null);
    try {
      await adjustConfidence(concept.id, delta);
      onChanged();
    } catch (e) {
      setWriteError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  async function toggleStatus() {
    if (busy) return;
    setBusy(true);
    setWriteError(null);
    try {
      await setStatus(
        concept.id,
        concept.status === "learning" ? "learned" : "learning",
      );
      onChanged();
    } catch (e) {
      setWriteError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  async function doDelete() {
    if (busy) return;
    setBusy(true);
    setWriteError(null);
    try {
      // 노트 쪽 역참조부터 — 실패해도 개념은 살아 있어 다시 시도할 수 있다
      if (sourceNote) {
        try {
          await removeNoteConcept(sourceNote.noteRel, concept.id);
        } catch {
          /* 노트가 이미 없거나 루트가 바뀌었을 뿐 — 개념 삭제를 막지 않는다 */
        }
      }
      // 파일 먼저, DB 나중 — 반대로 하면 실패 시 화면에 안 보이는 고아 디렉터리가 남는다
      await deleteConceptDir(concept.ulid);
      await deleteConcept(concept.id);
      onChanged({ deleted: true });
    } catch (e) {
      setWriteError(errMsg(e));
      setConfirmingDelete(false);
    } finally {
      setBusy(false);
    }
  }

  if (editing) {
    return (
      <div>
        <div className="field">
          <label>{t("concepts.field.title")}</label>
          <input
            className="input"
            value={dTitle}
            onChange={(e) => setDTitle(e.target.value)}
          />
        </div>
        <div className="field">
          <label>{t("concepts.field.summary")}</label>
          <textarea
            className="textarea"
            style={{ fontFamily: "var(--font)" }}
            rows={2}
            value={dSummary}
            onChange={(e) => setDSummary(e.target.value)}
          />
        </div>
        <div className="field">
          <label>{t("concepts.field.tags")}</label>
          <input
            className="input"
            value={dTags}
            onChange={(e) => setDTags(e.target.value)}
          />
        </div>
        <div className="field">
          <label>{t("concepts.field.detail")}</label>
          <textarea
            className="textarea"
            rows={18}
            value={dBody}
            onChange={(e) => setDBody(e.target.value)}
          />
        </div>
        {writeError && (
          <div className="error-note">
            {t("concepts.writeError", { err: writeError })}
          </div>
        )}
        <div className="detail-actions">
          <button className="btn btn-primary btn-sm" onClick={save} disabled={busy}>
            {busy ? t("concepts.saving") : t("common.save")}
          </button>
          <button
            className="btn btn-sm"
            onClick={() => setEditing(false)}
            disabled={busy}
          >
            {t("common.cancel")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <>
      <div>
        <div className="detail-head">
          <h1 className="detail-title">{concept.title}</h1>
          <ConfidenceDots value={concept.confidence} />
        </div>
        <div style={{ marginBottom: 8 }}>
          <StatusBadge status={concept.status} />
        </div>
        <p className="detail-summary">{concept.summary}</p>
        {concept.tags.length > 0 && (
          <div className="detail-tags">
            {concept.tags.map((t) => (
              <span className="chip" key={t}>
                #{t}
              </span>
            ))}
          </div>
        )}

        <div className="detail-actions detail-actions-split">
          <div className="detail-actions-group">
            <button
              className={`btn btn-sm ${concept.status === "learning" ? "btn-primary" : ""}`}
              onClick={toggleStatus}
              disabled={busy}
            >
              {concept.status === "learning" ? (
                <>
                  <Icon name="check" size={14} />
                  {t("concepts.action.markLearned")}
                </>
              ) : (
                <>
                  <Icon name="undo" size={14} />
                  {t("concepts.action.backToLearning")}
                </>
              )}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => changeConfidence(1)}
              disabled={busy || concept.confidence >= 3}
            >
              {t("concepts.field.confidence")}
              <Icon name="plus" size={13} />
            </button>
            <button
              className="btn btn-sm"
              onClick={() => changeConfidence(-1)}
              disabled={busy || concept.confidence <= 1}
            >
              {t("concepts.field.confidence")}
              <Icon name="minus" size={13} />
            </button>
            <button
              className="btn btn-sm"
              onClick={startEdit}
              disabled={busy || loadingBody || !!readError}
            >
              <Icon name="pencil" size={14} />
              {t("concepts.action.edit")}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => setAugmenting(true)}
              disabled={
                busy ||
                loadingBody ||
                !!readError ||
                missingFile ||
                !config?.provider
              }
              title={t("concepts.action.augmentTitle")}
            >
              <Icon name="sparkles" size={14} />
              {t("concepts.action.augment")}
            </button>
          </div>
          <div className="detail-actions-group">
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => setConfirmingDelete(true)}
              disabled={busy}
            >
              <Icon name="trash" size={14} />
              {t("common.delete")}
            </button>
          </div>
        </div>

        {writeError && (
          <div className="error-note">
            {t("concepts.writeError", { err: writeError })}
          </div>
        )}

        {loadingBody ? (
          <Spinner />
        ) : readError ? (
          <div className="error-note">{t("concepts.readError", { err: readError })}</div>
        ) : missingFile ? (
          <div className="hint">{t("concepts.missingFile")}</div>
        ) : (
          <div className="markdown">
            <Markdown>{body}</Markdown>
          </div>
        )}

        {sourceNote && (
          <div className="concept-source">
            <Tooltip
              label={
                otherRoot ? t("concepts.sourceNote.otherRoot") : sourceNote.anchor
              }
            >
              <button
                className="btn btn-sm"
                onClick={() => openNoteInApp(sourceNote.noteRel)}
                disabled={otherRoot}
              >
                <Icon name="book" size={13} />
                {t("concepts.sourceNote.open")}
              </button>
            </Tooltip>
            <span className="concept-source-name">
              {otherRoot
                ? `${rootDisplayName("notes", sourceNote.root!)} / ${sourceNote.noteRel}`
                : sourceNote.noteRel}
            </span>
          </div>
        )}

        <div className="detail-meta">
          {t("concepts.meta", {
            created: new Date(concept.created_at).toLocaleDateString(dateLocale()),
            updated: timeAgo(concept.updated_at),
            seen: concept.seen_count,
          })}
        </div>
      </div>

      <Modal
        open={confirmingDelete}
        title={t("concepts.delete.title")}
        narrow
        onClose={() => setConfirmingDelete(false)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setConfirmingDelete(false)}
              disabled={busy}
            >
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={doDelete}
              disabled={busy}
            >
              {busy ? t("concepts.delete.deleting") : t("common.delete")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          {delConfirm[0]}
          <b>{concept.title}</b>
          {delConfirm[1]}
          <br />
          {t("concepts.delete.irreversible")}
        </p>
      </Modal>

      <AugmentModal
        open={augmenting}
        concept={concept}
        currentBody={body}
        config={config}
        onClose={() => setAugmenting(false)}
        onApplied={(newBody) => {
          setBody(newBody);
          setAugmenting(false);
          onChanged();
        }}
      />
    </>
  );
}
