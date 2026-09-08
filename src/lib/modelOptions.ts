// 설정의 모델 선택지 — 큐레이션 목록(config.ts)과 CLI 카탈로그를 합친다.
//
// Codex 는 자기 카탈로그(~/.codex/models_cache.json, Rust `codex_models`)를 내주므로 그걸 쓰고, Claude 는
// CLI 가 모델 목록을 내주지 않아 큐레이션만 있다. 어느 쪽이든 목록에 없는 id 는 설정의 '직접 입력'으로 받는다 —
// 새 모델이 나왔을 때 앱 업데이트를 기다리지 않아도 되게. 빈 id 는 "CLI 기본 모델"(--model 미전달)이다.

export interface ModelOption {
  id: string;
  label: string;
  description?: string;
}

/** Select 에서 '직접 입력…' 을 고른 상태를 나타내는 가짜 값 — 실제 모델 id 로 저장되지 않는다 */
export const CUSTOM_MODEL = "__custom__";

const DESC_MAX = 56;

/** 카탈로그가 있으면 그것(+ CLI 기본), 없으면 큐레이션 그대로 */
export function mergeModelOptions(
  curated: { id: string; label: string }[],
  dynamic: ModelOption[] | null,
  cliDefaultLabel: string,
): { id: string; label: string }[] {
  if (!dynamic || dynamic.length === 0) return curated;
  const list = dynamic.map((m) => ({
    id: m.id,
    label: m.description ? `${m.label} — ${shorten(m.description, DESC_MAX)}` : m.label,
  }));
  return [...list, { id: "", label: cliDefaultLabel }];
}

function shorten(s: string, max: number): string {
  const t = s.trim();
  return t.length > max ? `${t.slice(0, max - 1)}…` : t;
}
