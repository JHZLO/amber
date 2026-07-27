// 설정 › 데일리 리포트 섹션. 연동 소스 활성화 + 드래그 순위(배열 순서=rank) + 소스별 확장 설정.
// 테마처럼 "변경 즉시 저장"(saveReportConfig).
// P1: GitHub · AI 세션. P2: Slack · Notion(MCP) — claude 에 등록된 서버를 선택해 재사용.

import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import type { McpServer, ReportSourceId } from "../types";
import { loadConfig, type AppConfig } from "../lib/config";
import {
  detectReportTools,
  loadReportConfig,
  reportGhAccounts,
  reportMcpServers,
  saveReportConfig,
  type GhAccount,
  type ReportConfig,
  type ReportTools,
} from "../lib/report";
import { Checkbox, Select, Spinner } from "../ui";
import { Icon } from "../icons";

const ALL_SOURCES: ReportSourceId[] = ["github", "ai_sessions", "slack", "notion"];
const MCP_SOURCES: ReportSourceId[] = ["slack", "notion"];
const LABEL: Record<ReportSourceId, string> = {
  github: "GitHub",
  ai_sessions: "AI 세션",
  slack: "Slack",
  notion: "Notion",
};
const SUB: Record<ReportSourceId, string> = {
  github: "내 계정 활동 이력",
  ai_sessions: "로컬 세션 요약",
  slack: "MCP · 메시지·스레드",
  notion: "MCP · 페이지·코멘트",
};

const MCP_STATUS_KO: Record<string, string> = {
  connected: "연결됨",
  needs_auth: "인증 필요",
  failed: "연결 실패",
  pending: "승인 대기",
  unknown: "",
};

export function ReportSettings() {
  const [cfg, setCfg] = useState<ReportConfig | null>(null);
  const [appCfg, setAppCfg] = useState<AppConfig | null>(null);
  const [tools, setTools] = useState<ReportTools | null>(null);
  const [ghAccounts, setGhAccounts] = useState<GhAccount[] | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [mcpServers, setMcpServers] = useState<McpServer[] | null>(null);
  const [mcpLoading, setMcpLoading] = useState(false);
  const [expanded, setExpanded] = useState<ReportSourceId | null>(null);
  const [dragId, setDragId] = useState<ReportSourceId | null>(null);
  const alive = useRef(true);

  const isClaude = appCfg?.provider === "claude";

  useEffect(() => {
    alive.current = true;
    // 설정에 이 섹션이 보이면 '설정을 거쳤다'고 보고 최초 게이트를 해제한다.
    loadReportConfig().then((c) => {
      if (!alive.current) return;
      const next = c.onboarded ? c : { ...c, onboarded: true };
      setCfg(next);
      if (!c.onboarded) void saveReportConfig(next);
    });
    loadConfig().then((ac) => {
      if (!alive.current) return;
      setAppCfg(ac);
      if (ac.provider === "claude") void redetectMcp(ac.cliPath);
    });
    void redetect();
    return () => {
      alive.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 서버 목록이 오면 이름이 맞는 연결된 서버를 자동 제안(비어 있을 때만)
  useEffect(() => {
    if (!mcpServers || !cfg) return;
    let next = cfg;
    if (!next.slackServer) {
      const m = mcpServers.find((s) => s.connected && /slack/i.test(s.name));
      if (m) next = { ...next, slackServer: m.name };
    }
    if (!next.notionServer) {
      const m = mcpServers.find((s) => s.connected && /notion/i.test(s.name));
      if (m) next = { ...next, notionServer: m.name };
    }
    if (next !== cfg) update(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mcpServers]);

  async function redetect() {
    setDetecting(true);
    try {
      const t = await detectReportTools();
      if (alive.current) setTools(t);
      // gh 로그인 계정 목록도 함께 (여러 계정일 때 조회 계정 선택용)
      const accts = await reportGhAccounts(t.gh?.path ?? null);
      if (alive.current) setGhAccounts(accts);
    } finally {
      if (alive.current) setDetecting(false);
    }
  }

  async function redetectMcp(cliPath?: string) {
    setMcpLoading(true);
    try {
      const path = cliPath ?? appCfg?.cliPath ?? "";
      const list = await reportMcpServers(path || null);
      if (alive.current) setMcpServers(list);
    } finally {
      if (alive.current) setMcpLoading(false);
    }
  }

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
      setCfg((prev) => {
        if (prev) void saveReportConfig(prev);
        return prev;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  if (!cfg) return null;

  const rows = cfg.sources.filter((s) => ALL_SOURCES.includes(s.id));
  const enabledOrder = cfg.sources.filter((s) => s.enabled).map((s) => s.id);
  const rankOf = (id: ReportSourceId) => enabledOrder.indexOf(id) + 1;

  function statusFor(id: ReportSourceId): { label: string; ok: boolean } {
    if (id === "github") {
      return tools?.gh
        ? { label: `gh ${tools.gh.version.replace(/^gh version\s*/, "")}`, ok: true }
        : { label: "gh 설치 필요", ok: false };
    }
    if (id === "ai_sessions") {
      const has = !!(tools?.claude_sessions || tools?.codex_sessions);
      return has ? { label: "감지됨", ok: true } : { label: "세션 없음", ok: false };
    }
    // slack / notion (MCP)
    if (!isClaude) return { label: "claude 필요", ok: false };
    const server = id === "slack" ? cfg!.slackServer : cfg!.notionServer;
    if (!server) return { label: "서버 선택", ok: false };
    // 감지 전(null)은 '아직 모름' — 실패(빈 배열)와 구분한다. 저장은 됐는데 "미확인" 으로 보이면
    // 설정이 날아간 걸로 오해한다(claude mcp list 는 10초 이상 걸린다).
    if (!mcpServers) return { label: "확인 중…", ok: false };
    const s = mcpServers.find((x) => x.name === server);
    if (!s) return { label: "미확인", ok: false };
    return s.connected
      ? { label: "연결됨", ok: true }
      : { label: MCP_STATUS_KO[s.status] || "미연결", ok: false };
  }

  function serverOptions() {
    const opts = [{ value: "", label: "선택 안 함" }];
    for (const s of mcpServers ?? []) {
      const st = MCP_STATUS_KO[s.status];
      opts.push({ value: s.name, label: s.connected ? s.name : `${s.name} · ${st}` });
    }
    return opts;
  }

  const hasConnectedMcp = (mcpServers ?? []).some((s) => s.connected);

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

      <div className="rep-src-list">
        {rows.map((s) => {
          const st = statusFor(s.id);
          const isOpen = expanded === s.id;
          const isMcp = MCP_SOURCES.includes(s.id);
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
                    {isMcp && <span className="rep-src-tag">MCP</span>}
                  </span>
                  <span className="rep-src-sub">{SUB[s.id]}</span>
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
                      {ghAccounts && ghAccounts.length > 0 && (
                        <div className="field">
                          <label>조회 계정</label>
                          <Select
                            block
                            value={cfg.githubAccount}
                            options={[
                              { value: "", label: "활성 계정 (기본)" },
                              ...ghAccounts.map((a) => ({
                                value: a.login,
                                label: a.active ? `${a.login} · 활성` : a.login,
                              })),
                            ]}
                            onChange={(v) => update({ ...cfg, githubAccount: v })}
                          />
                          <div className="hint">
                            gh 에 계정이 여러 개면 리포트에 쓸 계정을 고르세요. 그 계정으로 조회해요(전역 활성 계정은 안 바뀜).
                          </div>
                        </div>
                      )}
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
                  ) : s.id === "ai_sessions" ? (
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
                  ) : (
                    // slack / notion (MCP)
                    <McpSourceBody
                      id={s.id}
                      isClaude={isClaude}
                      loading={mcpLoading}
                      servers={mcpServers}
                      hasConnected={hasConnectedMcp}
                      value={s.id === "slack" ? cfg.slackServer : cfg.notionServer}
                      options={serverOptions()}
                      onPick={(v) =>
                        update(
                          s.id === "slack"
                            ? { ...cfg, slackServer: v }
                            : { ...cfg, notionServer: v },
                        )
                      }
                      onRedetect={() => void redetectMcp()}
                    />
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function McpSourceBody({
  id,
  isClaude,
  loading,
  servers,
  hasConnected,
  value,
  options,
  onPick,
  onRedetect,
}: {
  id: ReportSourceId;
  isClaude: boolean;
  loading: boolean;
  servers: McpServer[] | null;
  hasConnected: boolean;
  value: string;
  options: { value: string; label: string }[];
  onPick: (v: string) => void;
  onRedetect: () => void;
}) {
  if (!isClaude) {
    return (
      <div className="hint">
        {LABEL[id]} 수집은 claude 프로바이더 전용이에요. 설정 상단 <b>AI 연결</b>에서 claude 에
        연결하면 등록된 MCP 서버를 그대로 사용합니다.
      </div>
    );
  }
  return (
    <>
      <div className="field" style={{ marginBottom: 8 }}>
        <label>MCP 서버</label>
        {/* servers === null = 아직 감지 전. 여기서 '없어요' 안내를 띄우면 등록된 서버가 사라진 걸로 보인다 */}
        {loading || !servers ? (
          <div className="loading-box" style={{ padding: "12px 0" }}>
            <Spinner />
            <span className="hint">등록된 서버를 찾는 중…</span>
          </div>
        ) : servers && servers.length && hasConnected ? (
          <Select block value={value} options={options} onChange={onPick} />
        ) : (
          <div className="rep-mcp-guide">
            <div className="hint" style={{ marginBottom: 8 }}>
              claude 에 등록·인증된 MCP 서버가 없어요. 터미널에서 한 번만 등록·인증하면 이후 자동
              재사용돼요:
            </div>
            <code className="rep-mcp-cmd">
              claude mcp add --transport http {id} https://mcp.{id}.com/mcp
            </code>
            <code className="rep-mcp-cmd">claude → /mcp → 브라우저 인증</code>
          </div>
        )}
        <div className="hint" style={{ marginTop: 6 }}>
          인증은 claude CLI 가 관리해요. Amber 는 토큰을 저장하지 않습니다.
        </div>
      </div>
      <button className="btn btn-sm" onClick={onRedetect}>
        <Icon name="refresh" size={13} />
        서버 다시 감지
      </button>
    </>
  );
}
