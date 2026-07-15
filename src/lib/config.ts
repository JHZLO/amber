// 앱 설정 (settings 테이블에 key/value 로 저장). claude 경로/모델 등.

import { getSetting, setSetting } from "./db";

export interface AppConfig {
  claudePath: string;
  model: string;
}

export const DEFAULTS: AppConfig = {
  claudePath: "/opt/homebrew/bin/claude",
  model: "claude-opus-4-8",
};

export const MODELS = [
  { id: "claude-opus-4-8", label: "Opus (품질 우선)" },
  { id: "claude-sonnet-5", label: "Sonnet (속도·비용 절약)" },
];

export async function loadConfig(): Promise<AppConfig> {
  return {
    claudePath: (await getSetting("claude_path")) ?? DEFAULTS.claudePath,
    model: (await getSetting("claude_model")) ?? DEFAULTS.model,
  };
}

export async function saveConfig(c: AppConfig): Promise<void> {
  await setSetting("claude_path", c.claudePath.trim() || DEFAULTS.claudePath);
  await setSetting("claude_model", c.model || DEFAULTS.model);
}
