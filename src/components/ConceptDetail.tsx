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
import { deleteConceptDir, readNote, writeNote } from "../lib/vault";
import { ConfidenceDots, Modal, Spinner, StatusBadge, timeAgo } from "../ui";
import { Icon } from "../icons";
import { AugmentModal } from "./AugmentModal";

function parseTags(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((t) => t.trim().replace(/^#/, ""))
    .filter(Boolean);
}

export function ConceptDetail({
  concept,
  config,
  onChanged,
}: {
  concept: ConceptWithTags;
  config: AppConfig | null;
  onChanged: (opts?: { deleted?: boolean }) => void;
}) {
  const [body, setBody] = useState("");
  const [loadingBody, setLoadingBody] = useState(true);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [augmenting, setAugmenting] = useState(false);

  const [dTitle, setDTitle] = useState(concept.title);
  const [dSummary, setDSummary] = useState(concept.summary);
  const [dTags, setDTags] = useState(concept.tags.join(", "));
  const [dBody, setDBody] = useState("");

  useEffect(() => {
    let alive = true;
    setEditing(false);
    setLoadingBody(true);
    readNote(concept.detail_path)
      .then((b) => alive && setBody(b))
      .catch(() => alive && setBody("_(본문을 읽을 수 없습니다)_"))
      .finally(() => alive && setLoadingBody(false));
    return () => {
      alive = false;
    };
  }, [concept.id, concept.detail_path]);

  function startEdit() {
    setDTitle(concept.title);
    setDSummary(concept.summary);
    setDTags(concept.tags.join(", "));
    setDBody(body);
    setEditing(true);
  }

  async function save() {
    if (busy) return;
    setBusy(true);
    try {
      await updateConceptContent(concept.id, {
        title: dTitle.trim(),
        summary: dSummary.trim(),
      });
      await setConceptTags(concept.id, parseTags(dTags));
      await writeNote(concept.ulid, dBody);
      setBody(dBody);
      setEditing(false);
      onChanged();
    } finally {
      setBusy(false);
    }
  }

  async function changeConfidence(delta: number) {
    if (busy) return;
    setBusy(true);
    try {
      await adjustConfidence(concept.id, delta);
      onChanged();
    } finally {
      setBusy(false);
    }
  }

  async function toggleStatus() {
    if (busy) return;
    setBusy(true);
    try {
      await setStatus(
        concept.id,
        concept.status === "learning" ? "learned" : "learning",
      );
      onChanged();
    } finally {
      setBusy(false);
    }
  }

  async function doDelete() {
    if (busy) return;
    setBusy(true);
    try {
      await deleteConcept(concept.id);
      await deleteConceptDir(concept.ulid);
      onChanged({ deleted: true });
    } finally {
      setBusy(false);
    }
  }

  if (editing) {
    return (
      <div>
        <div className="field">
          <label>제목</label>
          <input
            className="input"
            value={dTitle}
            onChange={(e) => setDTitle(e.target.value)}
          />
        </div>
        <div className="field">
          <label>요약 (위젯 표시용)</label>
          <textarea
            className="textarea"
            style={{ fontFamily: "var(--font)" }}
            rows={2}
            value={dSummary}
            onChange={(e) => setDSummary(e.target.value)}
          />
        </div>
        <div className="field">
          <label>태그 (쉼표로 구분)</label>
          <input
            className="input"
            value={dTags}
            onChange={(e) => setDTags(e.target.value)}
          />
        </div>
        <div className="field">
          <label>상세 (Markdown)</label>
          <textarea
            className="textarea"
            rows={18}
            value={dBody}
            onChange={(e) => setDBody(e.target.value)}
          />
        </div>
        <div className="detail-actions">
          <button className="btn btn-primary btn-sm" onClick={save} disabled={busy}>
            {busy ? "저장 중…" : "저장"}
          </button>
          <button
            className="btn btn-sm"
            onClick={() => setEditing(false)}
            disabled={busy}
          >
            취소
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
                  학습완료
                </>
              ) : (
                <>
                  <Icon name="undo" size={14} />
                  다시 학습중
                </>
              )}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => changeConfidence(1)}
              disabled={busy || concept.confidence >= 3}
            >
              자신감
              <Icon name="plus" size={13} />
            </button>
            <button
              className="btn btn-sm"
              onClick={() => changeConfidence(-1)}
              disabled={busy || concept.confidence <= 1}
            >
              자신감
              <Icon name="minus" size={13} />
            </button>
            <button className="btn btn-sm" onClick={startEdit} disabled={busy}>
              <Icon name="pencil" size={14} />
              편집
            </button>
            <button
              className="btn btn-sm"
              onClick={() => setAugmenting(true)}
              disabled={busy || loadingBody || !config}
              title="현재 노트를 프롬프트로 AI가 보강"
            >
              <Icon name="sparkles" size={14} />
              AI 보강
            </button>
          </div>
          <div className="detail-actions-group">
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => setConfirmingDelete(true)}
              disabled={busy}
            >
              <Icon name="trash" size={14} />
              삭제
            </button>
          </div>
        </div>

        {loadingBody ? (
          <Spinner />
        ) : (
          <div className="markdown">
            <Markdown>{body}</Markdown>
          </div>
        )}

        <div className="detail-meta">
          추가 {new Date(concept.created_at).toLocaleDateString("ko-KR")} · 수정{" "}
          {timeAgo(concept.updated_at)} · 위젯 노출 {concept.seen_count}회
        </div>
      </div>

      <Modal
        open={confirmingDelete}
        title="개념 삭제"
        onClose={() => setConfirmingDelete(false)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setConfirmingDelete(false)}
              disabled={busy}
            >
              취소
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={doDelete}
              disabled={busy}
            >
              {busy ? "삭제 중…" : "삭제"}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          <b>{concept.title}</b> 개념을 삭제할까요?
          <br />
          되돌릴 수 없어요.
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
