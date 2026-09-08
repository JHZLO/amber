// AI 모달 공통 — 사용자가 친 지시와 체크한 지시(저장 프롬프트·빠른 지시)를 하나의 지시문으로 합친다.
//
// 칩을 누를 때 텍스트를 입력칸에 붙이지 않는 이유: 긴 저장 프롬프트가 칸을 덮어 내가 쓴 말이 묻히고,
// 한 번 붙으면 빼기도 어렵다. 그래서 선택은 상태(켜짐/꺼짐)로만 두고 보낼 때 여기서 합친다.
// 순서는 내가 친 말 → 저장 프롬프트 → 빠른 지시. 각 조각은 빈 줄로 나눈다 — 저장 프롬프트는
// 여러 줄(제목 구조 등)일 수 있어 한 줄 나열(· 구분)로는 깨진다.

/** 빈 조각은 버리고, 나머지를 순서대로 빈 줄로 잇는다 */
export function composeInstruction(typed: string, extras: readonly string[]): string {
  return [typed, ...extras]
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .join("\n\n");
}

/** 이어 쓰기 지시 — 잘린 결과의 끝 조각을 span 으로 넘기며 함께 보낸다. 조각은 그대로 두고 뒤를 잇게 해서
 *  되끼울 때 앞부분이 한 글자도 바뀌지 않는다(출력 언어는 lang_directive 가 따로 정한다). */
export const CONTINUE_INSTRUCTION =
  "This fragment is the tail of a note whose generation was cut off by an output limit. " +
  "Return the fragment unchanged, then continue writing from exactly where it stops until the note reaches a natural end: " +
  "finish the current section, then any sections the note's structure clearly still needs, then the closing. " +
  "Do not repeat or restate earlier parts and do not add a preamble.";

const TAIL_CHARS = 700;

/** 잘린 결과의 끝 조각 — 마지막 ~700자를 줄 시작에 맞춰 잡는다. 줄 중간에서 자르면 모델이 반쪽 줄을 문맥으로 오해한다 */
export function tailSpan(text: string, max = TAIL_CHARS): string {
  if (text.length <= max) return text;
  const from = text.length - max;
  const nl = text.lastIndexOf("\n", from);
  // 앞쪽에 줄바꿈이 없으면(한 덩어리) 그냥 max 만큼
  return nl === -1 || nl === 0 ? text.slice(from) : text.slice(nl + 1);
}
