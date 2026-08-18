// 설정 › 데일리 리포트 섹션. 연동 소스 활성화 + 드래그 순위(배열 순서=rank) + 소스별 확장 설정.
// 테마처럼 "변경 즉시 저장"(saveReportConfig).
// P1: GitHub · AI 세션. P2: Slack · Notion(MCP) — claude 에 등록된 서버를 선택해 재사용.

import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import type { McpServer, ReportSourceId } from "../types";
import { loadConfig, type AppConfig } from "../lib/config";
import {
  detectReportTools,
  loadMcpCache,
  loadReportConfig,
  reportGhAccounts,
  reportMcpServers,
  saveMcpCache,
  saveReportConfig,
  type GhAccount,
  type ReportConfig,
  type ReportTools,
} from "../lib/report";
import { Checkbox, Select, Spinner, Tooltip } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

const ALL_SOURCES: ReportSourceId[] = ["github", "ai_sessions", "slack", "notion"];
const MCP_SOURCES: ReportSourceId[] = ["slack", "notion"];
// GitHub·Slack·Notion 은 브랜드명이라 번역하지 않는다
const LABEL: Record<ReportSourceId, string> = {
  github: "GitHub",
  ai_sessions: t("report.source.aiSessions"),
  slack: "Slack",
  notion: "Notion",
};
const SUB: Record<ReportSourceId, string> = {
  github: t("report.sub.github"),
  ai_sessions: t("report.sub.aiSessions"),
  slack: t("report.sub.slack"),
  notion: t("report.sub.notion"),
};

const MCP_STATUS: Record<string, string> = {
  connected: t("report.mcpStatus.connected"),
  needs_auth: t("report.mcpStatus.needsAuth"),
  failed: t("report.mcpStatus.failed"),
  pending: t("report.mcpStatus.pending"),
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
  const suggested = useRef(false);

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
    loadConfig().then(async (ac) => {
      if (!alive.current) return;
      setAppCfg(ac);
      if (ac.provider !== "claude") return;
      // 지난 감지 결과를 먼저 붙여 즉시 상태를 보여주고(오래 걸리는 감지를 기다리지 않는다),
      // 곧바로 백그라운드 갱신을 돌려 최신 값으로 덮는다.
      const cached = await loadMcpCache();
      if (!alive.current) return;
      if (cached) setMcpServers(cached);
      void redetectMcp(ac.cliPath);
    });
    void redetect();
    return () => {
      alive.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 서버 목록·설정이 모두 준비되면 이름이 맞는 연결된 서버를 자동 제안(비어 있을 때만).
  // 캐시 덕에 목록이 cfg 보다 먼저 올 수 있어 둘 다 의존하되, 사용자가 '선택 안 함' 으로
  // 비운 걸 다시 채우지 않도록 열려 있는 동안 한 번만 돈다.
  useEffect(() => {
    if (suggested.current) return;
    if (!mcpServers || !cfg) return;
    suggested.current = true;
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
  }, [mcpServers, cfg]);

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
      if (!alive.current) return;
      setMcpServers(list);
      // 빈 결과는 캐시하지 않는다(감지 실패와 구분 불가) — 다음에 또 처음부터 기다리지 않게
      void saveMcpCache(list);
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
        : { label: t("report.status.ghMissing"), ok: false };
    }
    if (id === "ai_sessions") {
      const has = !!(tools?.claude_sessions || tools?.codex_sessions);
      return has
        ? { label: t("report.status.detected"), ok: true }
        : { label: t("report.status.noSessions"), ok: false };
    }
    // slack / notion (MCP)
    if (!isClaude) return { label: t("report.status.claudeRequired"), ok: false };
    const server = id === "slack" ? cfg!.slackServer : cfg!.notionServer;
    if (!server) return { label: t("report.status.pickServer"), ok: false };
    // 감지 전(null)은 '아직 모름' — 실패(빈 배열)와 구분한다. 저장은 됐는데 "미확인" 으로 보이면
    // 설정이 날아간 걸로 오해한다(claude mcp list 는 10초 이상 걸린다).
    if (!mcpServers) return { label: t("report.status.checking"), ok: false };
    const s = mcpServers.find((x) => x.name === server);
    if (!s) return { label: t("report.status.unknown"), ok: false };
    return s.connected
      ? { label: t("report.mcpStatus.connected"), ok: true }
      : { label: MCP_STATUS[s.status] || t("report.status.notConnected"), ok: false };
  }

  function serverOptions() {
    const opts = [{ value: "", label: t("report.mcp.serverNone") }];
    for (const s of mcpServers ?? []) {
      const st = MCP_STATUS[s.status];
      opts.push({ value: s.name, label: s.connected ? s.name : `${s.name} · ${st}` });
    }
    return opts;
  }

  const hasConnectedMcp = (mcpServers ?? []).some((s) => s.connected);

  return (
    <section className="set-section">
      <div className="set-head">
        <span className="set-eyebrow">{t("report.title")}</span>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={() => void redetect()} disabled={detecting}>
          <Icon name="refresh" size={13} />
          {detecting ? t("report.set.detecting") : t("report.set.redetect")}
        </button>
      </div>
      <p className="set-desc">{t("report.set.desc")}</p>

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
                  title={t("report.set.dragRank")}
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
                <Tooltip label={isOpen ? t("report.set.collapse") : t("report.set.expand")}>
                  <button
                    aria-label={isOpen ? t("report.set.collapse") : t("report.set.expand")}
                    className="icon-btn ghost sm"
                    onClick={() => setExpanded(isOpen ? null : s.id)}
                  >
                    <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={15} />
                  </button>
                </Tooltip>
              </div>

              {isOpen && (
                <div className="rep-src-body">
                  {s.id === "github" ? (
                    <>
                      {ghAccounts && ghAccounts.length > 0 && (
                        <div className="field">
                          <label>{t("report.gh.accountLabel")}</label>
                          <Select
                            block
                            value={cfg.githubAccount}
                            options={[
                              { value: "", label: t("report.gh.accountDefault") },
                              ...ghAccounts.map((a) => ({
                                value: a.login,
                                label: a.active
                                  ? `${a.login} · ${t("report.gh.accountActive")}`
                                  : a.login,
                              })),
                            ]}
                            onChange={(v) => update({ ...cfg, githubAccount: v })}
                          />
                          <div className="hint">{t("report.gh.accountHint")}</div>
                        </div>
                      )}
                      <div className="field">
                        <label>{t("report.gh.pathLabel")}</label>
                        <input
                          className="input"
                          value={cfg.githubPath}
                          placeholder={tools?.gh?.path ?? "/opt/homebrew/bin/gh"}
                          onChange={(e) => update({ ...cfg, githubPath: e.target.value })}
                        />
                        <div className="hint">{t("report.gh.pathHint")}</div>
                      </div>
                      <div className="field" style={{ marginBottom: 0 }}>
                        <label>{t("report.gh.repoLabel")}</label>
                        <input
                          className="input"
                          value={cfg.githubRepos.join(", ")}
                          placeholder={t("report.gh.repoPlaceholder")}
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
                        <div className="hint">{t("report.gh.repoHint")}</div>
                      </div>
                    </>
                  ) : s.id === "ai_sessions" ? (
                    <div className="rep-sub-toggles">
                      <Checkbox
                        checked={cfg.sessionsClaude}
                        onChange={() => update({ ...cfg, sessionsClaude: !cfg.sessionsClaude })}
                        label={t("report.sess.claude")}
                      />
                      <span className="rep-sub-label">
                        Claude Code{" "}
                        <span className="rep-sub-state">
                          {tools?.claude_sessions
                            ? t("report.status.detected")
                            : t("report.sess.noFolder")}
                        </span>
                      </span>
                      <Checkbox
                        checked={cfg.sessionsCodex}
                        onChange={() => update({ ...cfg, sessionsCodex: !cfg.sessionsCodex })}
                        label={t("report.sess.codex")}
                      />
                      <span className="rep-sub-label">
                        Codex{" "}
                        <span className="rep-sub-state">
                          {tools?.codex_sessions
                            ? t("report.status.detected")
                            : t("report.sess.noFolder")}
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
        {t("report.mcp.claudeOnlyPre", { name: LABEL[id] })}
        <b>{t("report.mcp.claudeOnlyLink")}</b>
        {t("report.mcp.claudeOnlyPost")}
      </div>
    );
  }
  return (
    <>
      <div className="field" style={{ marginBottom: 8 }}>
        <label>{t("report.mcp.serverLabel")}</label>
        {/* servers === null = 감지 전(캐시도 없음). 갱신 중에는 캐시를 계속 보여준다 —
            여기서 로딩/'없어요' 로 되돌리면 설정이 사라진 것처럼 보인다 */}
        {!servers ? (
          <div className="loading-box" style={{ padding: "12px 0" }}>
            <Spinner />
            <span className="hint">{t("report.mcp.searching")}</span>
          </div>
        ) : servers.length && hasConnected ? (
          <Select block value={value} options={options} onChange={onPick} />
        ) : (
          <div className="rep-mcp-guide">
            <div className="hint" style={{ marginBottom: 8 }}>
              {t("report.mcp.noneGuide")}
            </div>
            <code className="rep-mcp-cmd">
              claude mcp add --transport http {id} https://mcp.{id}.com/mcp
            </code>
            <code className="rep-mcp-cmd">{t("report.mcp.authStep")}</code>
          </div>
        )}
        <div className="hint" style={{ marginTop: 6 }}>
          {t("report.mcp.tokenHint")}
        </div>
      </div>
      <button className="btn btn-sm" onClick={onRedetect} disabled={loading}>
        <Icon name="refresh" size={13} />
        {loading ? t("report.set.detecting") : t("report.mcp.redetect")}
      </button>
    </>
  );
}
