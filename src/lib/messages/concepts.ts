// concepts 도메인 문자열 — 키는 "concepts." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).
// 대상: ConceptDetail · AddConceptModal · AugmentModal · PromoteConceptModal
// 주의: AI 에 실제로 보내는 지시문(프롬프트 재료)은 여기 넣지 않는다 — 화면에 렌더되는 것만.

const ko = {
  // 공용 폼 필드 (추가/편집/보강/승격 공유)
  "concepts.field.title": "제목",
  "concepts.field.summary": "요약 (위젯 표시용)",
  "concepts.field.tags": "태그 (쉼표로 구분)",
  "concepts.field.confidence": "자신감",
  "concepts.field.detail": "상세 (Markdown)",
  "concepts.field.detailNote": "상세 노트 (Markdown)",
  "concepts.field.augmentedNote": "보강된 상세 노트 (Markdown)",
  "concepts.form.required": "제목과 요약은 필수예요.",
  "concepts.preview.show": "프리뷰",
  "concepts.preview.source": "소스 편집",
  "concepts.saving": "저장 중…",

  // 상세 화면 액션
  "concepts.action.markLearned": "학습완료",
  "concepts.action.backToLearning": "다시 학습중",
  "concepts.action.edit": "편집",
  "concepts.action.augment": "AI 보강",
  "concepts.action.augmentTitle": "현재 노트를 프롬프트로 AI가 보강",
  "concepts.readError": "본문을 읽을 수 없어요 — {err}",
  "concepts.writeError": "저장하지 못했어요 — {err}",
  "concepts.missingFile": "상세 노트 파일이 없어요. 편집해서 저장하면 다시 만들어져요.",
  "concepts.sourceNote.open": "출처 노트 열기",
  "concepts.sourceNote.otherRoot": "다른 폴더의 노트예요 — 그 폴더로 바꾸면 열 수 있어요",
  "concepts.meta": "추가 {created} · 수정 {updated} · 위젯 노출 {seen}회",

  // 삭제 확인 — confirm 의 {title} 자리는 렌더 시 <b>로 감싸므로 반드시 남겨 둔다
  "concepts.delete.title": "개념 삭제",
  "concepts.delete.confirm": "{title} 개념을 삭제할까요?",
  "concepts.delete.irreversible": "되돌릴 수 없어요.",
  "concepts.delete.deleting": "삭제 중…",

  // 새 개념 추가
  "concepts.add.title": "새 개념 추가",
  "concepts.add.pasteLabel": "AI와 나눈 Q&A 원문을 붙여넣으세요",
  "concepts.add.pastePlaceholder": "여기에 대화 전체를 붙여넣기…",
  "concepts.add.charCount": "{n}자",
  "concepts.add.tooShort": "· 최소 20자 이상 입력하세요",
  "concepts.add.instructionLabel": "지시문 (선택) — Claude에게 정리 방향 지시",
  "concepts.add.instructionPlaceholder":
    "예: Postgres 관점 위주로 · 초보자도 이해하게 · 코드 예시 꼭 포함…",
  "concepts.add.instructionHint": "저장돼서 다음 추가 때도 기본값으로 채워져요.",
  "concepts.add.manual": "수동 작성",
  "concepts.add.generate": "Claude로 정리",
  "concepts.add.backToSource": "원문으로",
  "concepts.add.save": "저장 (학습중으로)",
  "concepts.add.thinking": "Claude가 정리하는 중…",
  "concepts.add.thinkingHint": "원문에서 핵심 개념 → 요약 → 상세 노트",
  "concepts.add.confidenceHint": "방금 배운 것은 1을 추천",

  // AI 보강
  "concepts.augment.title": "AI로 노트 보강",
  "concepts.augment.promptLabel": "어떻게 보강할까요? — Claude에게 지시",
  "concepts.augment.promptPlaceholder":
    "예: kube-proxy IPVS 모드 설정 예시를 코드블록으로 추가해줘 · 성능 비교 부분을 더 깊게 · 표로 정리해줘…",
  "concepts.augment.promptHint":
    "현재 노트 전체를 바탕으로 다시 정리해요. 자신감·학습상태는 그대로 유지돼요.",
  "concepts.augment.presets": "빠른 지시",
  "concepts.augment.preset.examples": "구체적인 예시·코드 추가",
  "concepts.augment.preset.deeper": "더 깊고 자세하게",
  "concepts.augment.preset.simpler": "초보자도 이해하게 쉽게",
  "concepts.augment.preset.concise": "핵심만 간결하게 압축",
  "concepts.augment.preset.pitfalls": "주의점·함정 보강",
  "concepts.augment.run": "AI로 보강",
  "concepts.augment.again": "다시 지시",
  "concepts.augment.apply": "적용",
  "concepts.augment.applying": "적용 중…",
  "concepts.augment.thinking": "Claude가 노트를 보강하는 중…",
  "concepts.augment.thinkingHint": "현재 노트 + 지시 → 보강된 상세 노트",
  "concepts.augment.loadingConfig": "설정을 불러오는 중이에요",

  // 노트 → 개념 승격
  "concepts.promote.title": "개념으로 만들기",
  "concepts.promote.thinking": "선택한 내용을 개념 카드로 정리하는 중…",
  "concepts.promote.thinkingHint": "선택 부분을 중심으로 요약·상세를 만들어요",
  "concepts.promote.save": "개념으로 저장 (학습중)",
} as const;

const en: Record<keyof typeof ko, string> = {
  "concepts.field.title": "Title",
  "concepts.field.summary": "Summary (shown in widget)",
  "concepts.field.tags": "Tags (comma separated)",
  "concepts.field.confidence": "Confidence",
  "concepts.field.detail": "Details (Markdown)",
  "concepts.field.detailNote": "Detail note (Markdown)",
  "concepts.field.augmentedNote": "Augmented note (Markdown)",
  "concepts.form.required": "Title and summary are required.",
  "concepts.preview.show": "Preview",
  "concepts.preview.source": "Edit source",
  "concepts.saving": "Saving…",

  "concepts.action.markLearned": "Mark learned",
  "concepts.action.backToLearning": "Back to learning",
  "concepts.action.edit": "Edit",
  "concepts.action.augment": "AI augment",
  "concepts.action.augmentTitle": "Let AI expand this note, using it as the prompt",
  "concepts.readError": "Couldn't read the note — {err}",
  "concepts.writeError": "Couldn't save — {err}",
  "concepts.missingFile": "The detail note file is missing. Edit and save to recreate it.",
  "concepts.sourceNote.open": "Open source note",
  "concepts.sourceNote.otherRoot": "This note lives in another folder — switch to it to open",
  "concepts.meta": "Added {created} · Edited {updated} · Shown in widget {seen}×",

  "concepts.delete.title": "Delete Concept",
  "concepts.delete.confirm": "Delete the concept {title}?",
  "concepts.delete.irreversible": "This can't be undone.",
  "concepts.delete.deleting": "Deleting…",

  "concepts.add.title": "Add Concept",
  "concepts.add.pasteLabel": "Paste a Q&A transcript from your AI chat",
  "concepts.add.pastePlaceholder": "Paste the whole conversation here…",
  "concepts.add.charCount": "{n} chars",
  "concepts.add.tooShort": "· enter at least 20 characters",
  "concepts.add.instructionLabel": "Instruction (optional) — guide how Claude organizes it",
  "concepts.add.instructionPlaceholder":
    "e.g. Focus on Postgres · keep it beginner-friendly · always include code examples…",
  "concepts.add.instructionHint": "Saved and prefilled the next time you add one.",
  "concepts.add.manual": "Write manually",
  "concepts.add.generate": "Organize with Claude",
  "concepts.add.backToSource": "Back to source",
  "concepts.add.save": "Save (as learning)",
  "concepts.add.thinking": "Claude is organizing…",
  "concepts.add.thinkingHint": "Key concept from the source → summary → detail note",
  "concepts.add.confidenceHint": "1 is recommended for things you just learned",

  "concepts.augment.title": "Augment Note with AI",
  "concepts.augment.promptLabel": "How should it be augmented? — instruct Claude",
  "concepts.augment.promptPlaceholder":
    "e.g. Add a kube-proxy IPVS config example as a code block · go deeper on the performance comparison · organize it into a table…",
  "concepts.augment.promptHint":
    "Rewrites based on the whole current note. Confidence and status stay unchanged.",
  "concepts.augment.presets": "Quick instructions",
  "concepts.augment.preset.examples": "Add concrete examples & code",
  "concepts.augment.preset.deeper": "Deeper and more detailed",
  "concepts.augment.preset.simpler": "Simple enough for beginners",
  "concepts.augment.preset.concise": "Compress to the essentials",
  "concepts.augment.preset.pitfalls": "Add caveats & pitfalls",
  "concepts.augment.run": "Augment with AI",
  "concepts.augment.again": "Instruct again",
  "concepts.augment.apply": "Apply",
  "concepts.augment.applying": "Applying…",
  "concepts.augment.thinking": "Claude is augmenting the note…",
  "concepts.augment.thinkingHint": "Current note + instruction → augmented detail note",
  "concepts.augment.loadingConfig": "Loading settings",

  "concepts.promote.title": "Promote to Concept",
  "concepts.promote.thinking": "Turning your selection into a concept card…",
  "concepts.promote.thinkingHint": "Builds the summary and details around your selection",
  "concepts.promote.save": "Save as concept (learning)",
} as const;

export const conceptsMessages = { ko, en };
