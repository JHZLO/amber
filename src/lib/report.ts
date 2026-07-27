// 데일리 리포트 프론트 계층.
// - Rust 브리지 래퍼(report_collect / report_generate / detect_report_tools)
// - 리포트 설정(소스 활성화·순위·gh 경로 등)을 settings KV 에 저장 (config.ts 패턴)
// - daily_reports 메타 CRUD + vault/reports/<date>.md 파일 R/W (본문 정본은 파일)
// - 투두 digest 빌더(DB → 마크다운). Rust 는 SQLite 미보유라 투두는 여기서 만든다.

import { invoke, Channel } from "@tauri-apps/api/core";
import {
  BaseDirectory,
  mkdir,
  writeTextFile,
  readTextFile,
  exists,
} from "@tauri-apps/plugin-fs";
import { getDb, getSetting, setSetting } from "./db";
import { listTodos, listOverdueOpen } from "./todos";
import { todayStr, formatDayShort } from "./date";
import type {
  CollectProgress,
  DailyReport,
  McpServer,
  McpSource,
  ReportSourceId,
  ReportSourcePref,
  SourceDigest,
  Todo,
} from "../types";

// ---- Rust 브리지 ----

export interface GhInfo {
  path: string;
  version: string;
}
export interface ReportTools {
  gh: GhInfo | null;
  claude_sessions: boolean;
  codex_sessions: boolean;
}

/** gh 설치/버전 + AI 세션 디렉터리 존재 여부 (설정 화면 상태 표시용) */
export function detectReportTools(): Promise<ReportTools> {
  return invoke<ReportTools>("detect_report_tools");
}

/** claude 에 등록된 MCP 서버 목록 (P2 Slack·Notion 선택용). claude 경로 필요 */
export function reportMcpServers(cliPath: string | null): Promise<McpServer[]> {
  return invoke<McpServer[]>("report_mcp_servers", { cliPath: cliPath ?? null });
}

// `claude mcp list` 는 등록 서버마다 health check 를 돌아 10초 이상 걸린다. 설정을 열 때마다
// 그만큼 "확인 중…" 을 보는 건 과하므로, 마지막 결과를 캐시해 즉시 보여주고 뒤에서 갱신한다
// (stale-while-revalidate). 캐시는 표시용일 뿐 — 실제 수집은 항상 claude 가 직접 한다.

const MCP_CACHE_KEY = "report_mcp_cache";

function isMcpServer(v: unknown): v is McpServer {
  const s = v as McpServer;
  return (
    !!s &&
    typeof s.name === "string" &&
    typeof s.connected === "boolean" &&
    typeof s.status === "string" &&
    typeof s.transport === "string"
  );
}

/** 마지막으로 감지한 MCP 서버 목록. 없거나 깨졌으면 null */
export async function loadMcpCache(): Promise<McpServer[] | null> {
  const raw = await getSetting(MCP_CACHE_KEY);
  if (!raw) return null;
  try {
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return null;
    const clean = arr.filter(isMcpServer);
    return clean.length ? clean : null;
  } catch {
    return null;
  }
}

export async function saveMcpCache(servers: McpServer[]): Promise<void> {
  // 빈 결과(감지 실패·타임아웃)로 멀쩡한 캐시를 덮지 않는다
  if (!servers.length) return;
  await setSetting(MCP_CACHE_KEY, JSON.stringify(servers));
}

/** gh 에 로그인된 계정 목록 (여러 계정일 때 조회 계정 선택용) */
export interface GhAccount {
  login: string;
  active: boolean;
}
export function reportGhAccounts(cliPath: string | null): Promise<GhAccount[]> {
  return invoke<GhAccount[]>("report_gh_accounts", { cliPath: cliPath ?? null });
}

export interface GithubCollectCfg {
  rank: number;
  path: string | null;
  repos: string[];
  /** 조회할 gh 계정 로그인 (null = 활성 계정) */
  account: string | null;
}
export interface SessionsCollectCfg {
  rank: number;
  claude: boolean;
  codex: boolean;
}

/** 활성 소스 병렬 수집. 소스별 완료는 onProgress 로 흘러온다. */
export function reportCollect(
  params: {
    date: string;
    startMs: number;
    endMs: number;
    tzOffsetMin: number;
    github: GithubCollectCfg | null;
    aiSessions: SessionsCollectCfg | null;
  },
  onProgress: (p: CollectProgress) => void,
): Promise<SourceDigest[]> {
  const channel = new Channel<CollectProgress>();
  channel.onmessage = onProgress;
  return invoke<SourceDigest[]>("report_collect", {
    date: params.date,
    startMs: params.startMs,
    endMs: params.endMs,
    tzOffsetMin: params.tzOffsetMin,
    github: params.github,
    aiSessions: params.aiSessions,
    onProgress: channel,
  });
}

export interface ReportGenResult {
  markdown: string;
  meta: {
    model: string;
    session_id: string | null;
    cost_usd: number | null;
    input_tokens: number | null;
    output_tokens: number | null;
    duration_ms: number | null;
  };
}

/** 투두 + digest → 리포트 마크다운(스트리밍). onDelta 로 생성 델타가 흘러온다.
 *  mcpSources 가 있으면(claude 전용) claude 가 등록 MCP 서버 도구를 직접 호출해 Slack·Notion 을 조회한다. */
export function reportGenerate(
  params: {
    date: string;
    todosDigest: string;
    digests: SourceDigest[];
    mcpSources?: McpSource[];
    model?: string | null;
    cliPath?: string | null;
    provider?: string | null;
    timeoutSecs?: number | null;
  },
  onDelta: (text: string) => void,
): Promise<ReportGenResult> {
  const channel = new Channel<string>();
  channel.onmessage = onDelta;
  return invoke<ReportGenResult>("report_generate", {
    date: params.date,
    todosDigest: params.todosDigest,
    digests: params.digests,
    mcpSources: params.mcpSources ?? [],
    model: params.model ?? null,
    cliPath: params.cliPath ?? null,
    provider: params.provider ?? null,
    timeoutSecs: params.timeoutSecs ?? null,
    onDelta: channel,
  });
}

// ---- 리포트 설정 (settings KV) ----

export interface ReportConfig {
  onboarded: boolean;
  /** 배열 순서 = 우선순위(rank). 앞일수록 리포트 중심 소스 */
  sources: ReportSourcePref[];
  githubPath: string;
  githubRepos: string[];
  /** 조회할 gh 계정 로그인 (빈 문자열 = 활성 계정). 여러 계정 로그인 시 선택 */
  githubAccount: string;
  sessionsClaude: boolean;
  sessionsCodex: boolean;
  /** P2 — 선택한 등록 MCP 서버 이름 (빈 문자열 = 미선택) */
  slackServer: string;
  notionServer: string;
}

/** 기본 소스 순서. github·ai_sessions 기본 on, slack·notion(P2)은 기본 off */
const DEFAULT_SOURCES: ReportSourcePref[] = [
  { id: "github", enabled: true },
  { id: "ai_sessions", enabled: true },
  { id: "slack", enabled: false },
  { id: "notion", enabled: false },
];

const VALID_IDS: ReportSourceId[] = ["github", "ai_sessions", "slack", "notion"];

function parseSources(raw: string | null): ReportSourcePref[] {
  if (!raw) return DEFAULT_SOURCES.map((s) => ({ ...s }));
  try {
    const arr = JSON.parse(raw) as ReportSourcePref[];
    const clean = arr.filter(
      (s) => s && VALID_IDS.includes(s.id) && typeof s.enabled === "boolean",
    );
    // 누락된 기본 소스는 뒤에 보충(마이그레이션 없이 새 소스 등장 흡수)
    for (const d of DEFAULT_SOURCES) {
      if (!clean.some((s) => s.id === d.id)) clean.push({ ...d });
    }
    return clean.length ? clean : DEFAULT_SOURCES.map((s) => ({ ...s }));
  } catch {
    return DEFAULT_SOURCES.map((s) => ({ ...s }));
  }
}

export async function loadReportConfig(): Promise<ReportConfig> {
  const [onb, sources, ghPath, ghRepos, ghAccount, sesClaude, sesCodex, slackSrv, notionSrv] =
    await Promise.all([
      getSetting("report_onboarded"),
      getSetting("report_sources"),
      getSetting("report_github_path"),
      getSetting("report_github_repos"),
      getSetting("report_github_account"),
      getSetting("report_sessions_claude"),
      getSetting("report_sessions_codex"),
      getSetting("report_slack_server"),
      getSetting("report_notion_server"),
    ]);
  return {
    onboarded: onb === "1",
    sources: parseSources(sources),
    githubPath: ghPath ?? "",
    githubRepos: (ghRepos ?? "")
      .split(",")
      .map((r) => r.trim())
      .filter(Boolean),
    githubAccount: ghAccount ?? "",
    // 기본 on (감지되면 사용). 명시적으로 "0" 저장했을 때만 off
    sessionsClaude: sesClaude !== "0",
    sessionsCodex: sesCodex !== "0",
    slackServer: slackSrv ?? "",
    notionServer: notionSrv ?? "",
  };
}

export async function saveReportConfig(c: ReportConfig): Promise<void> {
  await Promise.all([
    setSetting("report_onboarded", c.onboarded ? "1" : "0"),
    setSetting("report_sources", JSON.stringify(c.sources)),
    setSetting("report_github_path", c.githubPath.trim()),
    setSetting("report_github_repos", c.githubRepos.join(",")),
    setSetting("report_github_account", c.githubAccount.trim()),
    setSetting("report_sessions_claude", c.sessionsClaude ? "1" : "0"),
    setSetting("report_sessions_codex", c.sessionsCodex ? "1" : "0"),
    setSetting("report_slack_server", c.slackServer.trim()),
    setSetting("report_notion_server", c.notionServer.trim()),
  ]);
}

/** 활성 소스만, 배열 순서대로 rank(1..n) 부여 */
export function rankedSources(c: ReportConfig): { id: ReportSourceId; rank: number }[] {
  return c.sources
    .filter((s) => s.enabled)
    .map((s, i) => ({ id: s.id, rank: i + 1 }));
}

/** 활성 MCP 소스(Slack·Notion)를 report_generate 용 McpSource[] 로. 서버 미선택이면 제외 */
export function mcpSourcesFrom(c: ReportConfig): McpSource[] {
  const out: McpSource[] = [];
  for (const r of rankedSources(c)) {
    if (r.id === "slack" && c.slackServer)
      out.push({ id: "slack", rank: r.rank, server: c.slackServer });
    if (r.id === "notion" && c.notionServer)
      out.push({ id: "notion", rank: r.rank, server: c.notionServer });
  }
  return out;
}

// ---- daily_reports 메타 (DB) ----

export async function getReport(date: string): Promise<DailyReport | null> {
  const db = await getDb();
  const rows = await db.select<DailyReport[]>(
    `SELECT * FROM daily_reports WHERE report_date = $1`,
    [date],
  );
  return rows.length ? rows[0] : null;
}

export async function upsertReport(input: {
  date: string;
  filePath: string;
  sourcesJson: string;
  provider: string | null;
  model: string | null;
  durationMs: number | null;
}): Promise<void> {
  const db = await getDb();
  await db.execute(
    `INSERT INTO daily_reports (report_date, file_path, sources_json, provider, model, duration_ms, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7)
     ON CONFLICT(report_date) DO UPDATE SET
       file_path = excluded.file_path,
       sources_json = excluded.sources_json,
       provider = excluded.provider,
       model = excluded.model,
       duration_ms = excluded.duration_ms,
       updated_at = excluded.updated_at`,
    [
      input.date,
      input.filePath,
      input.sourcesJson,
      input.provider,
      input.model,
      input.durationMs,
      Date.now(),
    ],
  );
}

export async function deleteReport(date: string): Promise<void> {
  const db = await getDb();
  await db.execute(`DELETE FROM daily_reports WHERE report_date = $1`, [date]);
  const rel = `${VAULT}/${reportPathFor(date)}`;
  if (await exists(rel, { baseDir: BASE })) {
    await invoke("move_to_trash", { relPath: rel });
  }
}

// ---- vault/reports/<date>.md 파일 (본문 정본, frontmatter 없음) ----

const BASE = BaseDirectory.AppData;
const VAULT = "vault";

/** DB 에 저장할 상대경로 */
export function reportPathFor(date: string): string {
  return `reports/${date}.md`;
}

export async function writeReportFile(date: string, md: string): Promise<string> {
  await mkdir(`${VAULT}/reports`, { baseDir: BASE, recursive: true });
  const rel = reportPathFor(date);
  await writeTextFile(`${VAULT}/${rel}`, md, { baseDir: BASE });
  return rel;
}

export async function readReportFile(date: string): Promise<string | null> {
  const rel = `${VAULT}/${reportPathFor(date)}`;
  if (!(await exists(rel, { baseDir: BASE }))) return null;
  return readTextFile(rel, { baseDir: BASE });
}

// ---- 투두 digest (계획 축) ----

/** 선택 날짜의 할 일 + (오늘이면) 밀린 항목을 마크다운으로. Rust 생성 프롬프트의 '계획' 재료. */
export async function buildTodosDigest(date: string): Promise<{ md: string; count: number }> {
  const todos = await listTodos(date);
  const overdue = date === todayStr() ? await listOverdueOpen(date) : [];
  if (!todos.length && !overdue.length) return { md: "", count: 0 };

  const tops = todos.filter((t) => t.parent_id == null);
  const kids = (pid: number) => todos.filter((t) => t.parent_id === pid);
  const mark = (t: Todo) => (t.done === 1 ? "[x]" : "[ ]");

  const lines: string[] = [];
  for (const p of tops) {
    lines.push(`- ${mark(p)} ${p.content}`);
    for (const c of kids(p.id)) lines.push(`  - ${mark(c)} ${c.content}`);
  }
  if (overdue.length) {
    lines.push("", "밀린(미완료) 항목:");
    for (const t of overdue.slice(0, 20)) {
      lines.push(`- [ ] ${t.content} (원래 ${formatDayShort(t.due_date)})`);
    }
  }
  return { md: lines.join("\n"), count: todos.length };
}
