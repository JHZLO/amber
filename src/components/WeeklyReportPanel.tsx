// 주간 리포트 패널 — 캘린더 선택 단위가 '주'일 때만 마운트된다(TodoView 가 게이트).
// 그 단위로 바꾼 것 자체가 '이번 주를 보겠다'는 의사표시라 접힘 단계를 두지 않는다.
// 재료는 **그 주에 이미 만들어 둔 일간 리포트 본문**이다 (GitHub 재수집 없음):
//   ① 커버리지(7일 중 몇 일 있는지) → ② 스트리밍 → ③ 완성 → ④ 에러/재료 없음
// 출력 형식은 사용자의 `/Weekly Report` 스킬 규약(노션 공유용 중첩 목록)을 따른다 —
// 들여쓰기가 곧 계층이라 마크다운으로 렌더하지 않고 평문 그대로 그린다(§ 본문 렌더 주석).
// 정본: 본문 = vault/reports/<주 시작일>-week.md, 메타 = weekly_reports 테이블.

import { useEffect, useRef, useState } from "react";
import type { AppConfig } from "../lib/config";
import type { WeeklyReport } from "../types";
import {
  deleteWeeklyReport,
  getWeeklyReport,
  loadReportConfig,
  loadWeekSources,
  readWeeklyReportFile,
  type WeekDaySource,
} from "../lib/report";
import {
  cancelWeeklyReport,
  clearRun,
  isRunning,
  startWeeklyReport,
  useReportRun,
  weeklyKey,
  type RunPhase,
} from "../lib/reportRun";
import { formatDayShort, todayStr, weekDays } from "../lib/date";
import { AiThinking, Modal, Tooltip } from "../ui";
import { Icon } from "../icons";
import { t, dateLocale } from "../lib/i18n";
import { errText } from "../lib/errors";

function hhmm(ms: number): string {
  return new Date(ms).toLocaleTimeString(dateLocale(), {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/** 주 범위는 앱 전체와 같은 기준을 쓴다(lib/date.ts WEEK_STARTS_ON) — 캘린더·주 할 일·
 *  타임테이블 주간 뷰와 하루도 어긋나지 않는다. 그래도 헤더에 범위를 적어 두는 건,
 *  이 본문이 앱 밖(노션)으로 복사돼 나가는 것이라 어느 주인지가 본문과 함께 가야 해서다. */
export function WeeklyReportPanel({
  weekStart,
  config,
  active,
  onOpenSettings,
}: {
  /** 그 주 시작일 'YYYY-MM-DD' (lib/date weekStartOf) */
  weekStart: string;
  config: AppConfig | null;
  active: boolean;
  onOpenSettings: () => void;
}) {
  const key = weeklyKey(weekStart);
  const run = useReportRun(key);
  const [loaded, setLoaded] = useState<{ report: WeeklyReport; body: string } | null>(
    null,
  );
  const [sources, setSources] = useState<WeekDaySource[] | null>(null);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);
  const streamRef = useRef<HTMLPreElement>(null);

  const days = weekDays(weekStart);
  const weekEnd = days[6];
  const range = `${formatDayShort(weekStart)} – ${formatDayShort(weekEnd)}`;

  // 기존 리포트 로드 — 실행이 있으면 그게 정본이라 건드리지 않는다 (일간 패널과 같은 규칙)
  useEffect(() => {
    if (!active || run) return;
    let cancelled = false;
    void getWeeklyReport(weekStart)
      .then(async (r) => {
        // 본문은 행에 적힌 경로로 읽는다 — 키가 옮겨진 행은 파일명이 옛 키다(migrations/0013)
        const md = await readWeeklyReportFile(weekStart, r?.file_path);
        return { r, md };
      })
      .then(({ r, md }) => {
        if (cancelled) return;
        setLoaded(r && md != null ? { report: r, body: md } : null);
      });
    return () => {
      cancelled = true;
    };
  }, [active, weekStart, run]);

  // 주 뷰에 들어와 있을 때만 읽는다 — 일 뷰에서는 패널 자체가 마운트되지 않는다
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void loadWeekSources(weekStart).then((s) => {
      if (!cancelled) setSources(s);
    });
    return () => {
      cancelled = true;
    };
  }, [active, weekStart, run?.phase]);

  useEffect(() => {
    if (streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [run?.stream]);

  // 실패한 실행은 표시에서 배제한다 — 직전 리포트를 그대로 두고 실패는 모달로 알린다
  // (DailyReportPanel 과 같은 규약: 갈아끼우는 시점은 새 문서가 완성된 때뿐이다)
  const failed = run?.phase === "error";
  const shown = failed && loaded ? null : run;
  const phase: "idle" | RunPhase = shown?.phase ?? (loaded ? "done" : (run?.phase ?? "idle"));
  const stream = shown?.stream ?? "";
  const report = (shown ? shown.report : loaded?.report) as WeeklyReport | null;
  const body = shown ? shown.body : (loaded?.body ?? "");
  const failedWithPrev = failed && loaded ? (run?.error ?? null) : null;
  // 인라인 에러 노트는 **되돌릴 리포트가 없을 때만** — 있으면 위 모달이 맡는다.
  // shown?.error 로 쓰면 첫 실패(직전 리포트 없음)에서 shown 이 null 이라 에러가 통째로 사라진다.
  const error = failedWithPrev ? null : (run?.error ?? null);

  const have = sources?.filter((d) => !!d.body?.trim()) ?? null;
  const missing = sources?.filter((d) => !d.body?.trim()) ?? null;
  // 미래 주는 재료가 있을 수 없다. 이번 주는 진행 중이라 부분 요약이 되므로 막지 않고 알린다
  const isFutureWeek = weekStart > todayStr();
  const isCurrentWeek = !isFutureWeek && weekEnd >= todayStr();

  async function generate() {
    if (isRunning(key) || isFutureWeek) return;
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
    void startWeeklyReport(weekStart, weekEnd, config);
  }

  function regenerate() {
    setConfirmRegen(false);
    if (!config?.provider) {
      onOpenSettings();
      return;
    }
    setOpError(null);
    void startWeeklyReport(weekStart, weekEnd, config);
  }

  async function remove() {
    setConfirmDelete(false);
    try {
      await deleteWeeklyReport(weekStart);
      clearRun(key);
      setLoaded(null);
    } catch (e) {
      setOpError(errText(e));
    }
  }

  async function copy() {
    try {
      // 보이는 것이 곧 붙여넣을 것 — 평문 그대로 복사한다
      await navigator.clipboard.writeText(body);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 클립보드 실패는 조용히 무시 */
    }
  }

  const busy = phase === "streaming";

  return (
    <div className="report report-weekly">
      <div className="report-head">
        <span className="report-weekly-label">
          <span className="report-eyebrow">
            <Icon name="calendar-check" size={12} />
            {t("report.weekly.title")}
          </span>
          <span className="report-weekly-range">{range}</span>
          {busy && <span className="rail-busy" aria-label={t("report.weekly.busy")} />}
        </span>
        <span className="spacer" />
        {phase === "done" && (
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
            <Tooltip label={t("report.weekly.copy")}>
              <button
                aria-label={t("report.weekly.copy")}
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
        )}
        {(phase === "idle" || phase === "empty" || phase === "error") && (
          <button
            className="btn btn-sm btn-primary"
            onClick={() => void generate()}
            disabled={isFutureWeek || have?.length === 0}
            title={isFutureWeek ? t("report.weekly.futureNo") : undefined}
          >
            <Icon name="sparkles" size={13} />
            {config?.provider
              ? t("report.weekly.generate")
              : t("report.connectGenerate")}
          </button>
        )}
      </div>

      {/* ① 커버리지 — 어느 날이 재료로 들어가고 어느 날이 빠지는지 먼저 밝힌다.
          주간 요약이 조용히 빈약해지는 걸 막는 유일한 장치다. */}
      {phase !== "streaming" && sources && (
        <div className="report-week-days">
          {sources.map((d) => (
            <span
              key={d.date}
              className={`chip report-week-day ${d.body?.trim() ? "has" : "none"}`}
            >
              {d.weekday}
            </span>
          ))}
          <span className="report-week-count">
            {have && have.length > 0 ? (
              <>
                {t("report.weekly.coverage", { n: have.length })}
                {/* 빠진 날은 있을 때만 — 0일이라고 굳이 말하지 않는다 */}
                {!!missing?.length && (
                  <> {t("report.weekly.missingDays", { n: missing.length })}</>
                )}
              </>
            ) : (
              t("report.weekly.noSources")
            )}
          </span>
        </div>
      )}

      {isCurrentWeek && phase !== "done" && phase !== "streaming" && (
        <p className="report-hint">{t("report.weekly.currentWeek")}</p>
      )}

      {/* ② 스트리밍 */}
      {phase === "streaming" && (
        <>
          <div className="report-stop-row">
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => void cancelWeeklyReport(weekStart)}
              title={t("report.stopHint")}
            >
              <Icon name="x" size={13} />
              {t("report.stop")}
            </button>
          </div>
          {stream ? (
            <pre className="note-stream-body" ref={streamRef}>
              {stream}
              <span className="report-caret" aria-hidden="true" />
            </pre>
          ) : (
            <AiThinking label={t("report.weekly.summarizing")} indicator="ring" />
          )}
        </>
      )}

      {/* ③ 완성 — 노션에 붙일 원문이 중요하므로 렌더와 원문을 함께 볼 수 있게 둔다 */}
      {phase === "done" && (
        <>
          <div className="report-meta">
            {report && (
              <>
                <span className="chip report-src">
                  {t("report.weekly.builtFrom", {
                    n: safeCount(report.sources_json),
                  })}
                </span>
                <span className="report-stamp">
                  {hhmm(report.created_at)}
                  {report.model ? ` · ${report.model}` : ""}
                </span>
              </>
            )}
          </div>
          {/* 마크다운으로 렌더하지 않는다 — 이 형식은 '들여쓰기 + ㄴ' 으로 계층을 만드는
              평문이라, 마크다운을 태우면 줄바꿈이 사라져 한 문단으로 뭉개진다.
              보이는 그대로가 노션에 붙는 그대로여야 복사 버튼도 신뢰할 수 있다. */}
          <pre className="report-week-body">{body}</pre>
        </>
      )}

      {/* ④ 에러 / 재료 없음 */}
      {phase === "empty" && (
        <p className="report-hint report-empty">{t("report.weekly.empty")}</p>
      )}
      {phase === "error" && error && <div className="error-note">{error}</div>}
      {opError && <div className="error-note">{opError}</div>}

      <Modal
        open={confirmRegen}
        title={t("report.weekly.regenTitle")}
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
        <p>{t("report.weekly.regenBody")}</p>
      </Modal>

      {/* 다시 생성 실패 — 직전 리포트를 화면에 두고 실패만 알린다(닫으면 실행 상태 비움) */}
      <Modal
        open={!!failedWithPrev}
        title={t("report.failedTitle")}
        narrow
        onClose={() => clearRun(key)}
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => clearRun(key)}>
              {t("common.close")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>{failedWithPrev}</p>
        <p className="hint" style={{ marginBottom: 0 }}>{t("report.failedKept")}</p>
      </Modal>

      <Modal
        open={confirmDelete}
        title={t("report.weekly.deleteTitle")}
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
        <p>{t("report.weekly.deleteBody")}</p>
      </Modal>
    </div>
  );
}

/** sources_json = 묶은 날짜 배열. 깨졌으면 0 (표시용이라 던지지 않는다) */
function safeCount(raw: string): number {
  try {
    const v = JSON.parse(raw);
    return Array.isArray(v) ? v.length : 0;
  } catch {
    return 0;
  }
}
