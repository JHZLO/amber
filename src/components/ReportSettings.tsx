// 설정 › 데일리 리포트 섹션. 연동 소스 활성화 + 드래그 순위(배열 순서=rank) + 소스별 확장 설정.
// 테마처럼 "변경 즉시 저장"(saveReportConfig). P1 소스: GitHub · AI 세션 (Slack·Notion 은 P2).

import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import type { ReportSourceId } from "../types";
import {
  detectReportTools,
  loadReportConfig,
  saveReportConfig,
  type ReportConfig,
  type ReportTools,
} from "../lib/report";
import { Checkbox } from "../ui";
import { Icon } from "../icons";

const P1_SOURCES: ReportSourceId[] = ["github", "ai_sessions"];
const LABEL: Record<ReportSourceId, string> = {
  github: "GitHub",
  ai_sessions: "AI 세션",
  slack: "Slack",
  notion: "Notion",
};

export function ReportSettings() {
  const [cfg, setCfg] = useState<ReportConfig | null>(null);
  const [tools, setTools] = useState<ReportTools | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [expanded, setExpanded] = useState<ReportSourceId | null>(null);
  const [dragId, setDragId] = useState<ReportSourceId | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    // 설정 화면에 이 섹션이 보이면 '설정을 거쳤다'고 보고 최초 게이트를 해제한다.
    loadReportConfig().then((c) => {
      if (!alive.current) return;
      const next = c.onboarded ? c : { ...c, onboarded: true };
      setCfg(next);
      if (!c.onboarded) void saveReportConfig(next);
    });
    void redetect();
    return () => {
      alive.current = false;
    };
  }, []);

  async function redetect() {
    setDetecting(true);
    try {
      const t = await detectReportTools();
      if (alive.current) setTools(t);
    } finally {
      if (alive.current) setDetecting(false);
    }
  }

  // 상태 갱신 + 즉시 영속
  function update(next: ReportConfig) {
    setCfg(next);
    void saveReportConfig(next);
  }

  function toggleEnable(id: ReportSourceId) {
    if (!cfg) return;
    update({
      ...cfg,
      sources: cfg.sources.map((s) =>
        s.id === id ? { ...s, enabled: !s.enabled } : s,
      ),
    });
  }

  // 드래그 재배열 — 커서가 다른 행 중심을 넘으면 배열에서 위치 교환(라이브). 놓으면 이미 반영됨.
  function startDrag(e: ReactMouseEvent, id: ReportSourceId) {
    e.preventDefault();
    setDragId(id);
    document.body.classList.add("dragging-rows");
    const onMove = (ev: MouseEvent) => {
      const el = document
        .elementFromPoint(ev.clientX, ev.clientY)
        ?.closest<HTMLElement>("[data-src-row]");
      const overId = el?.dataset.srcId as ReportSourceId | undefined;
      if (!overId || overId === id) return;
      setCfg((prev) => {
        if (!prev) return prev;
        const arr = prev.sources.slice();
        const from = arr.findIndex((s) => s.id === id);
        const to = arr.findIndex((s) => s.id === overId);
        if (from === -1 || to === -1 || from === to) return prev;
        const [moved] = arr.splice(from, 1);
        arr.splice(to, 0, moved);
        return { ...prev, sources: arr };
      });
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("dragging-rows");
      setDragId(null);
      // 최종 순서 영속
      setCfg((prev) => {
        if (prev) void saveReportConfig(prev);
        return prev;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  if (!cfg) return null;

  // P1 소스만, cfg 순서대로. rank = 활성 소스 중 순번
  const rows = cfg.sources.filter((s) => P1_SOURCES.includes(s.id));
  const enabledOrder = cfg.sources.filter((s) => s.enabled).map((s) => s.id);
  const rankOf = (id: ReportSourceId) => enabledOrder.indexOf(id) + 1;

  function statusFor(id: ReportSourceId): { label: string; ok: boolean } {
    if (id === "github") {
      return tools?.gh
        ? { label: `gh ${tools.gh.version.replace(/^gh version\s*/, "")}`, ok: true }
        : { label: "gh 설치 필요", ok: false };
    }
    const has = !!(tools?.claude_sessions || tools?.codex_sessions);
    return has ? { label: "감지됨", ok: true } : { label: "세션 없음", ok: false };
  }

  return (
    <section className="set-section">
      <div className="set-head">
        <span className="set-eyebrow">데일리 리포트</span>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={() => void redetect()} disabled={detecting}>
          <Icon name="refresh" size={13} />
          {detecting ? "감지 중…" : "다시 감지"}
        </button>
      </div>
      <p className="set-desc">
        켠 플랫폼만 수집해요. 위에 둘수록 리포트의 중심이 됩니다(순위 = 행 순서, 드래그로 조정).
      </p>

      <div className="rep-src-list" ref={listRef}>
        {rows.map((s) => {
          const st = statusFor(s.id);
          const isOpen = expanded === s.id;
          return (
            <div
              key={s.id}
              className={`rep-src ${s.enabled ? "" : "off"} ${dragId === s.id ? "dragging" : ""}`}
              data-src-row
              data-src-id={s.id}
            >
              <div className="rep-src-head">
                <span
                  className="todo-grip rep-src-grip"
                  title="드래그해서 순위 변경"
                  aria-hidden="true"
                  onMouseDown={(e) => startDrag(e, s.id)}
                >
                  <Icon name="grip" size={14} />
                </span>
                <span className={`rep-rank ${s.enabled ? "on" : ""}`}>
                  {s.enabled ? rankOf(s.id) : "—"}
                </span>
                <Checkbox
                  checked={s.enabled}
                  onChange={() => toggleEnable(s.id)}
                  label={LABEL[s.id]}
                />
                <div className="rep-src-name">
                  <span className="rep-src-title">
                    {LABEL[s.id]}
                    {s.id === "ai_sessions" && <span className="rep-src-tag">Claude · Codex</span>}
                  </span>
                  <span className="rep-src-sub">
                    {s.id === "github" ? "내 계정 활동 이력" : "로컬 세션 요약"}
                  </span>
                </div>
                <span className={`rep-status ${st.ok ? "ok" : ""}`}>{st.label}</span>
                <button
                  className="icon-btn ghost sm"
                  title={isOpen ? "접기" : "펼치기"}
                  onClick={() => setExpanded(isOpen ? null : s.id)}
                >
                  <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={15} />
                </button>
              </div>

              {isOpen && (
                <div className="rep-src-body">
                  {s.id === "github" ? (
                    <>
                      <div className="field">
                        <label>gh CLI 경로</label>
                        <input
                          className="input"
                          value={cfg.githubPath}
                          placeholder={tools?.gh?.path ?? "/opt/homebrew/bin/gh"}
                          onChange={(e) => update({ ...cfg, githubPath: e.target.value })}
                        />
                        <div className="hint">
                          비우면 로그인 셸 PATH 에서 자동으로 찾아요. gh 자체 로그인을 그대로 사용합니다.
                        </div>
                      </div>
                      <div className="field" style={{ marginBottom: 0 }}>
                        <label>레포 필터 (선택)</label>
                        <input
                          className="input"
                          value={cfg.githubRepos.join(", ")}
                          placeholder="owner/repo, owner/other — 비우면 전체"
                          onChange={(e) =>
                            update({
                              ...cfg,
                              githubRepos: e.target.value
                                .split(",")
                                .map((r) => r.trim())
                                .filter(Boolean),
                            })
                          }
                        />
                        <div className="hint">지정하면 그 레포 활동만 모아 잡음을 줄여요.</div>
                      </div>
                    </>
                  ) : (
                    <div className="rep-sub-toggles">
                      <Checkbox
                        checked={cfg.sessionsClaude}
                        onChange={() => update({ ...cfg, sessionsClaude: !cfg.sessionsClaude })}
                        label="Claude Code 세션"
                      />
                      <span className="rep-sub-label">
                        Claude Code{" "}
                        <span className="rep-sub-state">
                          {tools?.claude_sessions ? "감지됨" : "세션 폴더 없음"}
                        </span>
                      </span>
                      <Checkbox
                        checked={cfg.sessionsCodex}
                        onChange={() => update({ ...cfg, sessionsCodex: !cfg.sessionsCodex })}
                        label="Codex 세션"
                      />
                      <span className="rep-sub-label">
                        Codex{" "}
                        <span className="rep-sub-state">
                          {tools?.codex_sessions ? "감지됨" : "세션 폴더 없음"}
                        </span>
                      </span>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>

      <p className="hint rep-soon">
        <Icon name="workflow" size={12} />
        Slack · Notion (MCP) 연동은 곧 지원돼요.
      </p>
    </section>
  );
}
