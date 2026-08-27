// 데일리 리포트 패널 — 할 일 체크리스트 아래(TodoView detail)에 사는 4상태 블록.
// ① 미생성(버튼) → ② 수집(소스별 칩) → 요약 스트리밍 → ③ 완성(마크다운+메타) / ④ 에러·활동없음.
// 생성 '실행'은 lib/reportRun 스토어가 백그라운드로 돌린다 — 이 컴포넌트는 구독·표시만 한다.
// 그래서 탭/날짜를 바꿔도 생성이 안 끊기고, 응답 올 때까지 로딩이 유지된다.
// 스트리밍 중엔 누적 텍스트를 그대로 보여주고(델타마다 마크다운 전체를 다시 파싱하지 않게)
// 끝난 뒤에 한 번만 마크다운으로 렌더한다 — NoteAiModal 과 같은 패턴.
// 정본: 본문 = vault/reports/<date>.md, 메타 = daily_reports 테이블 (lib/report.ts).

import { useEffect, useRef, useState } from "react";
import type { AppConfig } from "../lib/config";
import type { DailyReport } from "../types";
import {
  deleteReport,
  getReport,
  loadReportConfig,
  readReportFile,
  writeReportFile,
} from "../lib/report";
import {
  cancelReport,
  clearRun,
  isRunning,
  startReport,
  useReportRun,
  type RunChip,
  type RunPhase,
} from "../lib/reportRun";
import { todayStr } from "../lib/date";
import { Markdown } from "./Markdown";
import { AiThinking, Modal, Spinner, Tooltip, UnsavedModal } from "../ui";
import { Icon } from "../icons";
import { t, dateLocale } from "../lib/i18n";
import { errText } from "../lib/errors";

// GitHub·Slack·Notion 은 브랜드명이라 번역하지 않는다
const SRC_LABEL: Record<string, string> = {
  todos: t("report.source.todos"),
  github: "GitHub",
  ai_sessions: t("report.source.aiSessions"),
  slack: "Slack",
  notion: "Notion",
};

function hhmm(ms: number): string {
  return new Date(ms).toLocaleTimeString(dateLocale(), {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function DailyReportPanel({
  date,
  config,
  active,
  onOpenSettings,
}: {
  date: string;
  config: AppConfig | null;
  active: boolean;
  onOpenSettings: () => void;
}) {
  const run = useReportRun(date); // 백그라운드 실행 상태 (없으면 undefined)
  const [loaded, setLoaded] = useState<{ report: DailyReport; body: string } | null>(
    null,
  );
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);
  const streamRef = useRef<HTMLPreElement>(null);
  // 직접 수정 — 노트 편집과 같은 2분할(좌 원문 / 우 라이브 프리뷰). 정본은 파일이라
  // 저장은 writeReportFile 한 번이고, DB 메타(생성 시각·모델)는 건드리지 않는다.
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [savedFlash, setSavedFlash] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);

  const isFuture = date > todayStr();

  // 스토어에 실행이 없을 때만 디스크에서 기존 리포트 로드(실행이 있으면 그게 정본).
  useEffect(() => {
    if (!active || run) return;
    let cancelled = false;
    void Promise.all([getReport(date), readReportFile(date)]).then(([r, md]) => {
      if (cancelled) return;
      setLoaded(r && md != null ? { report: r, body: md } : null);
    });
    return () => {
      cancelled = true;
    };
  }, [active, date, run]);

  // 생성 중 새 텍스트가 오면 스트림 박스를 맨 아래로 자동 스크롤 (NoteAiModal 과 동일)
  useEffect(() => {
    if (streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [run?.stream]);

  // 표시값: 실행 중이면 스토어, 아니면 디스크 로드.
  //
  // **실패한 실행은 표시에서 배제한다.** 다시 생성이 실패했다고 그때까지 쓰던 문서가 화면에서
  // 사라지면 안 된다 — 사용량 한도처럼 내 잘못이 아닌 이유로도 실패하는데, 그 대가로 멀쩡한
  // 리포트를 잃는 셈이 된다. 갈아끼우는 시점은 **새 문서가 완성된 때** 하나뿐이다.
  // 실패는 모달로 알리고(닫으면 clearRun), 화면은 직전 리포트를 그대로 유지한다.
  const failed = run?.phase === "error";
  const shown = failed && loaded ? null : run;
  const phase: "idle" | RunPhase = shown?.phase ?? (loaded ? "done" : (run?.phase ?? "idle"));
  const chips: RunChip[] = shown?.chips ?? [];
  const stream = shown?.stream ?? "";
  const report = shown ? shown.report : (loaded?.report ?? null);
  const body = shown ? shown.body : (loaded?.body ?? "");
  const failedWithPrev = failed && loaded ? (run?.error ?? null) : null;
  // 인라인 에러 노트는 **되돌릴 리포트가 없을 때만** — 있으면 위 모달이 맡는다.
  // shown?.error 로 쓰면 첫 실패(직전 리포트 없음)에서 shown 이 null 이라 에러가 통째로 사라진다.
  const error = failedWithPrev ? null : (run?.error ?? null);

  async function generate() {
    if (isRunning(date) || isFuture) return;
    // 게이트: AI 미연결 → 온보딩/설정, 리포트 최초 설정 안 함 → 설정 열기
    if (!config?.provider) {
      onOpenSettings();
      return;
    }
    const rc = await loadReportConfig();
    if (!rc.onboarded) {
      onOpenSettings();
      return;
    }
    setOpError(null);
    void startReport(date, config); // 백그라운드 실행 (스토어가 상태 관리)
  }

  function regenerate() {
    setConfirmRegen(false);
    if (!config?.provider) {
      onOpenSettings();
      return;
    }
    setOpError(null);
    void startReport(date, config);
  }

  async function remove() {
    setConfirmDelete(false);
    try {
      await deleteReport(date);
      clearRun(date);
      setLoaded(null);
    } catch (e) {
      setOpError(errText(e));
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(body);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 클립보드 실패는 조용히 무시 */
    }
  }

  function startEdit() {
    setDraft(body);
    setOpError(null);
    setEditing(true);
  }

  const dirty = editing && draft !== body;

  /** 편집 종료 — 고친 게 있으면 확인을 받는다(초안은 파일에 없으니 닫으면 사라진다) */
  function closeEdit() {
    if (dirty) {
      setConfirmDiscard(true);
      return;
    }
    setEditing(false);
  }

  async function saveEdit() {
    if (!editing) return;
    const md = draft;
    try {
      await writeReportFile(date, md);
      // 스토어에 실행이 남아 있으면 그쪽 body 가 표시 정본이라 방금 저장한 내용이 가려진다 —
      // 저장 뒤로는 파일이 정본이므로 실행 상태를 비우고 로드본으로 갈아탄다.
      // 메타는 DB 에서 다시 읽는다(run.report 는 일간·주간 공용 타입이라 그대로 못 쓴다).
      const meta = await getReport(date);
      if (meta) setLoaded({ report: meta, body: md });
      clearRun(date);
      setEditing(false);
      setSavedFlash(true);
      setTimeout(() => setSavedFlash(false), 1500);
    } catch (e) {
      setOpError(errText(e));
    }
  }

  // ⌘S 저장 — 노트 편집과 같은 단축키(NotesView). 편집 중일 때만 가로챈다.
  const saveRef = useRef(saveEdit);
  saveRef.current = saveEdit;
  useEffect(() => {
    if (!editing) return;
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void saveRef.current();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [editing]);

  const doneSources: { id: string; ok: boolean; items: number; mcp?: boolean }[] =
    report ? safeParse(report.sources_json) : [];

  return (
    <div className="report">
      <div className="report-head">
        <span className="report-eyebrow">
          <Icon name="sparkles" size={12} />
          {t("report.title")}
        </span>
        <span className="spacer" />
        {phase === "done" && editing ? (
          // 편집 중 헤더 — 되돌릴 수 없는 액션(재생성·삭제)은 감춘다. 초안이 열린 채로
          // 눌리면 방금 고친 게 조용히 사라진다.
          <span className="report-actions">
            <button className="btn btn-sm" onClick={closeEdit}>
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-sm btn-primary"
              onClick={() => void saveEdit()}
              disabled={!dirty}
            >
              {t("common.save")}
            </button>
          </span>
        ) : phase === "done" ? (
          <span className="report-actions">
            <Tooltip label={t("report.edit")}>
              <button
                aria-label={t("report.edit")}
                className="icon-btn sm"
                onClick={startEdit}
              >
                <Icon name="pencil" size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("report.regen")}>
              <button
                aria-label={t("report.regen")}
                className="icon-btn sm"
                onClick={() => setConfirmRegen(true)}
              >
                <Icon name="refresh" size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("report.copy")}>
              <button
                aria-label={t("report.copy")}
                className="icon-btn sm"
                onClick={() => void copy()}
              >
                <Icon name={copied ? "check" : "copy"} size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("common.delete")}>
              <button
                aria-label={t("common.delete")}
                className="icon-btn sm danger"
                onClick={() => setConfirmDelete(true)}
              >
                <Icon name="trash" size={13} />
              </button>
            </Tooltip>
          </span>
        ) : phase === "idle" || phase === "empty" || phase === "error" ? (
          <button
            className="btn btn-sm btn-primary"
            onClick={() => void generate()}
            disabled={isFuture}
            title={isFuture ? t("report.futureNo") : undefined}
          >
            <Icon name="sparkles" size={13} />
            {config?.provider ? t("report.generate") : t("report.connectGenerate")}
          </button>
        ) : null}
      </div>

      {/* ① 미생성 */}
      {phase === "idle" && (
        <p className="report-hint">
          {isFuture ? t("report.hintFuture") : t("report.hintIdle")}
        </p>
      )}

      {/* ② 수집/스트리밍 — 소스 칩 + 진행/미리보기 */}
      {(phase === "collecting" || phase === "streaming") && (
        <>
          <div className="report-chips">
            {chips.map((c) => {
              const chip = (
                <span key={c.id} className={`report-chip ${c.status}`}>
                  {c.status === "ok" && <Icon name="check" size={12} />}
                  {c.status === "pending" && <Spinner />}
                  {c.status === "error" && <Icon name="x" size={12} />}
                  {c.status === "mcp" && <Icon name="workflow" size={12} />}
                  {SRC_LABEL[c.id] ?? c.id}
                  {c.status === "ok" && ` ${c.items}`}
                </span>
              );
              // 실패 사유는 백엔드가 준 유일한 실행 가능한 안내다 — 빨간 칩만 남기지 않는다
              return c.error ? (
                <Tooltip key={c.id} label={c.error}>
                  {chip}
                </Tooltip>
              ) : (
                chip
              );
            })}
            {/* 중단은 소스 칩과 **같은 줄** 우측 — 진행 중인 그 작업에 딸린 조작이라
                줄을 따로 내면 무엇을 멈추는 버튼인지 연결이 끊긴다. */}
            <button
              className="btn btn-sm btn-danger-ghost report-stop"
              onClick={() => void cancelReport(date)}
              disabled={phase === "collecting"}
              title={t("report.stopHint")}
            >
              <Icon name="x" size={13} />
              {t("report.stop")}
            </button>
          </div>
          {phase === "collecting" ? (
            // 수집은 gh·로컬 세션 파일을 훑는 단계라 '찾는 중'이 실제 동작에 가깝다
            <AiThinking label={t("report.collecting")} compact icon="search" />
          ) : stream ? (
            <pre className="note-stream-body" ref={streamRef}>
              {stream}
              <span className="report-caret" aria-hidden="true" />
            </pre>
          ) : (
            <AiThinking label={t("report.summarizing")} compact />
          )}
        </>
      )}

      {/* ③ 완성 */}
      {phase === "done" && (
        <>
          <div className="report-meta">
            {doneSources
              .filter((s) => s.ok && (s.items > 0 || s.mcp))
              .map((s) => (
                <span key={s.id} className="chip report-src">
                  #{SRC_LABEL[s.id] ?? s.id}
                </span>
              ))}
            {doneSources
              .filter((s) => !s.ok)
              .map((s) => (
                <span
                  key={s.id}
                  className="chip report-src failed"
                  title={t("report.srcFailedTitle")}
                >
                  {t("report.srcFailed", { name: SRC_LABEL[s.id] ?? s.id })}
                </span>
              ))}
            {report && (
              <span className="report-stamp">
                {hhmm(report.created_at)}
                {report.model ? ` · ${report.model}` : ""}
              </span>
            )}
          </div>
          {editing ? (
            // 좌 원문 / 우 라이브 프리뷰 — 노트 편집 모드와 같은 문법(.claude/DESIGN.md §7)
            <>
              <div className="report-edit-split">
                <textarea
                  className="textarea report-edit-src"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  spellCheck={false}
                  autoFocus
                />
                <div className="markdown note-preview report-body">
                  <Markdown>{draft}</Markdown>
                </div>
              </div>
              <p className="report-hint">{t("report.editHint")}</p>
            </>
          ) : (
            <div className="markdown report-body">
              <Markdown>{body}</Markdown>
            </div>
          )}
        </>
      )}

      {/* ④ 에러 / 활동 없음 */}
      {phase === "empty" && (
        <p className="report-hint report-empty">{t("report.empty")}</p>
      )}
      {phase === "error" && error && <div className="error-note">{error}</div>}
      {opError && <div className="error-note">{opError}</div>}
      {savedFlash && <div className="ok-note">{t("report.editSaved")}</div>}

      {/* 다시 생성 실패 — 직전 리포트는 화면에 그대로 두고 실패만 알린다.
          닫으면 실행 상태를 비워 디스크 리포트가 정본으로 남는다. */}
      <Modal
        open={!!failedWithPrev}
        title={t("report.failedTitle")}
        narrow
        onClose={() => clearRun(date)}
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => clearRun(date)}>
              {t("common.close")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>{failedWithPrev}</p>
        <p className="hint" style={{ marginBottom: 0 }}>{t("report.failedKept")}</p>
      </Modal>

      {/* 초안 버리기 확인 — 노트·다이어그램과 같은 공용 모달(ui.tsx) */}
      <UnsavedModal
        open={confirmDiscard}
        onKeep={() => setConfirmDiscard(false)}
        onDiscard={() => {
          setConfirmDiscard(false);
          setEditing(false);
        }}
      />

      <Modal
        open={confirmRegen}
        title={t("report.regenTitle")}
        onClose={() => setConfirmRegen(false)}
        narrow
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => setConfirmRegen(false)}>
              {t("common.cancel")}
            </button>
            <button className="btn btn-danger-ghost" onClick={regenerate}>
              {t("report.regen")}
            </button>
          </>
        }
      >
        <p>{t("report.regenBody")}</p>
      </Modal>

      <Modal
        open={confirmDelete}
        title={t("report.deleteTitle")}
        onClose={() => setConfirmDelete(false)}
        narrow
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => setConfirmDelete(false)}>
              {t("common.cancel")}
            </button>
            <button className="btn btn-danger-ghost" onClick={() => void remove()}>
              {t("common.delete")}
            </button>
          </>
        }
      >
        <p>{t("report.deleteBody")}</p>
      </Modal>
    </div>
  );
}

function safeParse(
  json: string,
): { id: string; ok: boolean; items: number; mcp?: boolean }[] {
  try {
    const arr = JSON.parse(json);
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}
