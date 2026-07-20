// 데일리 리포트 패널 — 할 일 체크리스트 아래(TodoView detail)에 사는 4상태 블록.
// ① 미생성(버튼) → ② 수집(소스별 칩) → 요약 스트리밍 → ③ 완성(마크다운+메타) / ④ 에러·활동없음.
// 정본: 본문 = vault/reports/<date>.md, 메타 = daily_reports 테이블 (lib/report.ts).

import { useCallback, useEffect, useRef, useState } from "react";
import type { AppConfig } from "../lib/config";
import type { CollectProgress, DailyReport, SourceDigest } from "../types";
import {
  buildTodosDigest,
  deleteReport,
  getReport,
  loadReportConfig,
  rankedSources,
  readReportFile,
  reportCollect,
  reportGenerate,
  upsertReport,
  writeReportFile,
} from "../lib/report";
import { dayRangeMs, todayStr } from "../lib/date";
import { friendlyError } from "../lib/claude";
import { Markdown } from "./Markdown";
import { AiThinking, Modal, Spinner } from "../ui";
import { Icon } from "../icons";

const errMsg = (e: unknown) => friendlyError(e);

type Phase = "idle" | "collecting" | "streaming" | "done" | "empty" | "error";

const SRC_LABEL: Record<string, string> = {
  todos: "투두",
  github: "GitHub",
  ai_sessions: "AI 세션",
  slack: "Slack",
  notion: "Notion",
};

type ChipStatus = "ok" | "pending" | "error";
interface Chip {
  id: string;
  status: ChipStatus;
  items: number;
}

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
  const [phase, setPhase] = useState<Phase>("idle");
  const [report, setReport] = useState<DailyReport | null>(null);
  const [body, setBody] = useState<string>("");
  const [stream, setStream] = useState<string>("");
  const [chips, setChips] = useState<Chip[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copied, setCopied] = useState(false);

  const busyRef = useRef(false);
  const isFuture = date > todayStr();

  // 날짜 변경 시 기존 리포트 로드 (생성 중이면 건드리지 않음)
  const load = useCallback(async () => {
    if (busyRef.current) return;
    try {
      const [r, md] = await Promise.all([getReport(date), readReportFile(date)]);
      if (r && md != null) {
        setReport(r);
        setBody(md);
        setPhase("done");
      } else {
        setReport(null);
        setBody("");
        setPhase("idle");
      }
      setStream("");
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [date]);

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  async function generate() {
    if (busyRef.current || isFuture) return;

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

    busyRef.current = true;
    setError(null);
    setStream("");
    setPhase("collecting");
    try {
      const { md: todosDigest, count } = await buildTodosDigest(date);
      const ranked = rankedSources(rc);
      const githubR = ranked.find((s) => s.id === "github");
      const sessR = ranked.find((s) => s.id === "ai_sessions");

      // 칩 초기화: 투두(즉시 확정) + 활성 플랫폼(수집 대기)
      const initChips: Chip[] = [{ id: "todos", status: "ok", items: count }];
      for (const s of ranked) {
        if (s.id === "github" || s.id === "ai_sessions") {
          initChips.push({ id: s.id, status: "pending", items: 0 });
        }
      }
      setChips(initChips);

      const [startMs, endMs] = dayRangeMs(date);
      const digests = await reportCollect(
        {
          date,
          startMs,
          endMs,
          tzOffsetMin: new Date().getTimezoneOffset(),
          github: githubR
            ? {
                rank: githubR.rank,
                path: rc.githubPath || null,
                repos: rc.githubRepos,
              }
            : null,
          aiSessions: sessR
            ? { rank: sessR.rank, claude: rc.sessionsClaude, codex: rc.sessionsCodex }
            : null,
        },
        (p: CollectProgress) => {
          setChips((prev) =>
            prev.map((c) =>
              c.id === p.id
                ? { id: c.id, status: p.ok ? "ok" : "error", items: p.items }
                : c,
            ),
          );
        },
      );

      // 활동 전무 → 생성 없이 종료(크레딧 절약)
      const hasActivity = count > 0 || digests.some((d) => d.ok && d.items > 0);
      if (!hasActivity) {
        setPhase("empty");
        return;
      }

      setPhase("streaming");
      const { markdown, meta } = await reportGenerate(
        {
          date,
          todosDigest,
          digests,
          model: config.model,
          cliPath: config.cliPath,
          provider: config.provider,
        },
        (t: string) => setStream((s) => s + t),
      );

      const filePath = await writeReportFile(date, markdown);
      const sourcesJson = JSON.stringify(
        digests.map((d: SourceDigest) => ({
          id: d.id,
          rank: d.rank,
          ok: d.ok,
          items: d.items,
          error: d.error,
        })),
      );
      await upsertReport({
        date,
        filePath,
        sourcesJson,
        provider: config.provider,
        model: meta.model,
        durationMs: meta.duration_ms,
      });
      setBody(markdown);
      setReport(await getReport(date));
      setPhase("done");
    } catch (e) {
      setError(errMsg(e));
      setPhase("error");
    } finally {
      busyRef.current = false;
    }
  }

  async function remove() {
    setConfirmDelete(false);
    try {
      await deleteReport(date);
      setReport(null);
      setBody("");
      setPhase("idle");
    } catch (e) {
      setError(errMsg(e));
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

  const doneSources: { id: string; ok: boolean; items: number }[] = report
    ? safeParse(report.sources_json)
    : [];

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
                {SRC_LABEL[c.id] ?? c.id}
                {c.status === "ok" && c.id !== "todos" && ` ${c.items}`}
                {c.status === "ok" && c.id === "todos" && ` ${c.items}`}
              </span>
            ))}
          </div>
          {phase === "collecting" ? (
            <AiThinking label="활동을 모으는 중…" compact />
          ) : stream ? (
            <div className="markdown report-body streaming">
              <Markdown>{stream}</Markdown>
              <span className="report-caret" aria-hidden="true" />
            </div>
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
              .filter((s) => s.ok && s.items > 0)
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
            <button
              className="btn btn-primary"
              onClick={() => {
                setConfirmRegen(false);
                void generate();
              }}
            >
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

function safeParse(json: string): { id: string; ok: boolean; items: number }[] {
  try {
    const arr = JSON.parse(json);
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}
