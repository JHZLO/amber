// 앱 설정 (settings 테이블에 key/value 로 저장).
// AI 는 특정 벤더에 묶지 않는다 — 온보딩/설정에서 감지·연결한 프로바이더(claude/codex/gemini)를
// ai_provider 로 저장하고, 경로/모델은 프로바이더별 키로 보관한다.

import { getSetting, setSetting } from "./db";

export type AiProvider = "claude" | "codex" | "gemini";

export const PROVIDER_LABELS: Record<AiProvider, string> = {
  claude: "Claude Code",
  codex: "OpenAI Codex CLI",
  gemini: "Gemini CLI",
};

/** 프로바이더별 모델 선택지. 빈 id = CLI 기본 모델 사용(설정 파일의 model 값을 따름) */
export const PROVIDER_MODELS: Record<AiProvider, { id: string; label: string }[]> = {
  claude: [
    { id: "claude-opus-5", label: "Opus 5 (최신·품질 우선)" },
    { id: "claude-opus-4-8", label: "Opus 4.8 (품질 우선)" },
    { id: "claude-sonnet-5", label: "Sonnet 5 (균형)" },
    { id: "claude-haiku-4-5-20251001", label: "Haiku 4.5 (속도·비용 절약)" },
  ],
  codex: [
    { id: "gpt-5.6-sol", label: "GPT-5.6 (최신)" },
    { id: "gpt-5.5", label: "GPT-5.5" },
    { id: "", label: "CLI 기본 모델" },
  ],
  gemini: [{ id: "", label: "CLI 기본 모델" }],
};

export interface AppConfig {
  /** 연결된 AI. null = 미연결 (AI 기능 비활성) */
  provider: AiProvider | null;
  /** 온보딩(연결 또는 건너뛰기)을 마쳤는지 — 최초 1회만 온보딩 노출 */
  onboarded: boolean;
  /** 활성 프로바이더 CLI 의 절대경로 */
  cliPath: string;
  /** 활성 프로바이더의 모델 id (빈 문자열 = CLI 기본) */
  model: string;
}

const DEFAULT_CLAUDE_MODEL = "claude-opus-4-8";

const isProvider = (v: string | null): v is AiProvider =>
  v === "claude" || v === "codex" || v === "gemini";

const pathKey = (p: AiProvider) => `ai_path_${p}`;
const modelKey = (p: AiProvider) => `ai_model_${p}`;

export async function loadConfig(): Promise<AppConfig> {
  let provider: AiProvider | null = null;
  const saved = await getSetting("ai_provider");
  if (isProvider(saved)) provider = saved;

  let onboarded = (await getSetting("ai_onboarded")) === "1";

  // 구버전 마이그레이션: ai_provider 가 없지만 claude 설정이 있으면 claude 로 간주
  if (!provider && !onboarded) {
    const legacyPath = await getSetting("claude_path");
    if (legacyPath) {
      provider = "claude";
      onboarded = true;
      await setSetting("ai_provider", "claude");
      await setSetting("ai_onboarded", "1");
      await setSetting(pathKey("claude"), legacyPath);
      const legacyModel = await getSetting("claude_model");
      if (legacyModel) await setSetting(modelKey("claude"), legacyModel);
    }
  }

  if (!provider) {
    return { provider: null, onboarded, cliPath: "", model: "" };
  }

  const cliPath = (await getSetting(pathKey(provider))) ?? "";
  const model =
    (await getSetting(modelKey(provider))) ??
    (provider === "claude" ? DEFAULT_CLAUDE_MODEL : "");
  return { provider, onboarded, cliPath, model };
}

/** 프로바이더 연결 (온보딩/설정 공용). 경로·모델을 프로바이더별 키에 저장 */
export async function connectProvider(
  provider: AiProvider,
  cliPath: string,
  model?: string,
): Promise<AppConfig> {
  await setSetting("ai_provider", provider);
  await setSetting("ai_onboarded", "1");
  await setSetting(pathKey(provider), cliPath.trim());
  if (model !== undefined) await setSetting(modelKey(provider), model);
  return loadConfig();
}

/** AI 없이 사용 (온보딩 건너뛰기) */
export async function skipAiOnboarding(): Promise<AppConfig> {
  await setSetting("ai_onboarded", "1");
  return loadConfig();
}

/** 설정 화면 저장: 활성 프로바이더의 경로/모델 갱신 */
export async function saveConfig(c: AppConfig): Promise<void> {
  if (!c.provider) return;
  await setSetting("ai_provider", c.provider);
  await setSetting(pathKey(c.provider), c.cliPath.trim());
  await setSetting(modelKey(c.provider), c.model);
}
