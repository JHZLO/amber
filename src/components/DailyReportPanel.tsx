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
  clearRun,
  isRunning,
  startReport,
  useReportRun,
  type RunChip,
  type RunPhase,
} from "../lib/reportRun";
import { todayStr } from "../lib/date";
import { Markdown } from "./Markdown";
import { AiThinking, Modal, Spinner } from "../ui";
import { Icon } from "../icons";

const SRC_LABEL: Record<string, string> = {
  todos: "투두",
  github: "GitHub",
  ai_sessions: "AI 세션",
  slack: "Slack",
  notion: "Notion",
};

function hhmm(ms: number): string {
  return new Date(ms).toLocaleTimeString("ko-KR", {
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
      setOpError(e instanceof Error ? e.message : String(e));
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
          데일리 리포트
        </span>
        <span className="spacer" />
        {phase === "done" ? (
          <span className="report-actions">
            <button
              className="icon-btn sm"
              title="다시 생성"
              onClick={() => setConfirmRegen(true)}
            >
              <Icon name="refresh" size={13} />
            </button>
            <button className="icon-btn sm" title="복사" onClick={() => void copy()}>
              <Icon name={copied ? "check" : "copy"} size={13} />
            </button>
            <button
              className="icon-btn sm danger"
              title="삭제"
              onClick={() => setConfirmDelete(true)}
            >
              <Icon name="trash" size={13} />
            </button>
          </span>
        ) : phase === "idle" || phase === "empty" || phase === "error" ? (
          <button
            className="btn btn-sm btn-primary"
            onClick={() => void generate()}
            disabled={isFuture}
            title={isFuture ? "미래 날짜는 생성할 수 없어요" : undefined}
          >
            <Icon name="sparkles" size={13} />
            {config?.provider ? "리포트 생성" : "AI 연결하고 생성"}
          </button>
        ) : null}
      </div>

      {/* ① 미생성 */}
      {phase === "idle" && (
        <p className="report-hint">
          {isFuture
            ? "미래 날짜예요. 지난 날짜나 오늘을 선택해 생성하세요."
            : "투두와 연동 플랫폼 활동으로 하루를 정리해 드려요."}
        </p>
      )}

      {/* ② 수집/스트리밍 — 소스 칩 + 진행/미리보기 */}
      {(phase === "collecting" || phase === "streaming") && (
        <>
          <div className="report-chips">
            {chips.map((c) => (
              <span key={c.id} className={`report-chip ${c.status}`}>
                {c.status === "ok" && <Icon name="check" size={12} />}
                {c.status === "pending" && <Spinner />}
                {c.status === "error" && <Icon name="x" size={12} />}
                {c.status === "mcp" && <Icon name="workflow" size={12} />}
                {SRC_LABEL[c.id] ?? c.id}
                {c.status === "ok" && ` ${c.items}`}
              </span>
            ))}
          </div>
          {phase === "collecting" ? (
            <AiThinking label="활동을 모으는 중…" compact />
          ) : stream ? (
            <pre className="note-stream-body" ref={streamRef}>
              {stream}
              <span className="report-caret" aria-hidden="true" />
            </pre>
          ) : (
            <AiThinking label="요약하는 중…" compact />
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
                <span key={s.id} className="chip report-src failed" title="수집 실패">
                  {SRC_LABEL[s.id] ?? s.id} 실패
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
        <p className="report-hint report-empty">
          이 날짜엔 기록된 활동이 없어요. 투두를 체크하거나 다른 날짜를 골라보세요.
        </p>
      )}
      {phase === "error" && error && <div className="error-note">{error}</div>}
      {opError && <div className="error-note">{opError}</div>}

      <Modal
        open={confirmRegen}
        title="리포트 다시 생성"
        onClose={() => setConfirmRegen(false)}
        narrow
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => setConfirmRegen(false)}>
              취소
            </button>
            <button className="btn btn-primary" onClick={regenerate}>
              다시 생성
            </button>
          </>
        }
      >
        <p>기존 리포트를 새로 생성한 내용으로 덮어써요. 계속할까요?</p>
      </Modal>

      <Modal
        open={confirmDelete}
        title="리포트 삭제"
        onClose={() => setConfirmDelete(false)}
        narrow
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => setConfirmDelete(false)}>
              취소
            </button>
            <button className="btn btn-danger-ghost" onClick={() => void remove()}>
              삭제
            </button>
          </>
        }
      >
        <p>이 날짜의 리포트를 삭제해요. 되돌릴 수 없어요.</p>
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
