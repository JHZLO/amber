// 저장 프롬프트: 설정 화면에서 CRUD, NoteAiModal 이 읽어 칩으로 노출.
// settings 테이블에 JSON 배열 한 줄(key=note_ai_prompts)로 저장 → 스키마/마이그레이션 불필요.

import { getSetting, setSetting } from "./db";

export interface SavedPrompt {
  id: string;
  label: string; // 칩에 보일 짧은 이름
  text: string; // 실제로 삽입될 지시문
}

const KEY = "note_ai_prompts";

function newId(): string {
  // WKWebView 에서 사용 가능. 실패 시 시간+난수 폴백.
  try {
    return crypto.randomUUID();
  } catch {
    return `p_${Date.now()}_${Math.floor(Math.random() * 1e6)}`;
  }
}

/** 저장된 프롬프트 목록 (깨진 값은 걸러냄) */
export async function loadPrompts(): Promise<SavedPrompt[]> {
  const raw = await getSetting(KEY);
  if (!raw) return [];
  try {
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr
      .filter(
        (p): p is SavedPrompt =>
          p && typeof p.id === "string" && typeof p.text === "string",
      )
      .map((p) => ({ id: p.id, label: String(p.label ?? ""), text: p.text }));
  } catch {
    return [];
  }
}

export async function savePrompts(list: SavedPrompt[]): Promise<void> {
  await setSetting(KEY, JSON.stringify(list));
}

export function makePrompt(label = "", text = ""): SavedPrompt {
  return { id: newId(), label, text };
}
