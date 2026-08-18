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
import { AiThinking, Modal, Spinner, Tooltip } from "../ui";
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

  // 표시값: 실행 중이면 스토어, 아니면 디스크 로드
  const phase: "idle" | RunPhase = run?.phase ?? (loaded ? "done" : "idle");
  const chips: RunChip[] = run?.chips ?? [];
  const stream = run?.stream ?? "";
  const report = run ? run.report : (loaded?.report ?? null);
  const body = run ? run.body : (loaded?.body ?? "");
  const error = run?.error ?? null;

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
        {phase === "done" ? (
          <span className="report-actions">
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
          </div>
          <div className="report-stop">
            <button
              className="btn btn-sm"
              onClick={() => void cancelReport(date)}
              disabled={phase === "collecting"}
              title={t("report.stopHint")}
            >
              <Icon name="x" size={13} />
              {t("report.stop")}
            </button>
          </div>
          {phase === "collecting" ? (
            <AiThinking label={t("report.collecting")} compact />
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
          <div className="markdown report-body">
            <Markdown>{body}</Markdown>
          </div>
        </>
      )}

      {/* ④ 에러 / 활동 없음 */}
      {phase === "empty" && (
        <p className="report-hint report-empty">{t("report.empty")}</p>
      )}
      {phase === "error" && error && <div className="error-note">{error}</div>}
      {opError && <div className="error-note">{opError}</div>}

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
            <button className="btn btn-primary" onClick={regenerate}>
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
