// notes 도메인 문자열 — 키는 "notes." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).

const ko = {
  "notes.title": "필기노트",

  // 트리 헤더 툴바
  "notes.newNote": "새 노트",
  "notes.newFolder": "새 폴더",
  "notes.refresh": "새로고침",
  "notes.tooltip.newNote": "새 노트 · {dir}",
  "notes.tooltip.newFolder": "새 폴더 · {dir}",
  "notes.tooltip.refresh": "새로고침 · Finder 변경 반영",

  // 트리 행 hover 액션
  "notes.row.newNoteHere": "이 폴더에 새 노트",
  "notes.row.newFolderHere": "이 폴더에 새 폴더",
  "notes.rename": "이름 변경",

  // 트리 빈 상태
  "notes.tree.empty.lead": "아직 노트가 없어요.",
  "notes.tree.empty.sub": "폴더로 분류하며 마크다운으로 기록해 보세요.",
  "notes.tree.firstNote": "첫 노트 만들기",

  // 상세 (읽기/편집)
  "notes.readError": "노트를 읽을 수 없어요 — {error}",
  "notes.saveCmd": "저장 (⌘S)",
  "notes.saving": "저장 중…",
  "notes.edit": "편집",
  "notes.aiWrite": "AI 작성",
  "notes.ai.fromDraftTip": "현재 초안을 바탕으로 AI가 작성/보강",
  "notes.ai.fromNoteTip": "현재 노트를 바탕으로 AI가 작성/보강",
  "notes.toc": "목차",
  "notes.madeConcepts": "이 노트에서 만든 개념",
  "notes.openConcept": "개념 열기 — “{anchor}”",
  "notes.meta.modified": "수정 {time}",
  "notes.meta.qcount.one": "질문 1개",
  "notes.meta.qcount.other": "질문 {n}개",

  // 상세 빈 상태
  "notes.empty.lead": "왼쪽에서 노트를 열거나, 새 노트를 만들어 기록을 시작하세요.",
  "notes.empty.sub": "폴더로 분류하고 마크다운으로 자유롭게 적을 수 있어요.",
  "notes.empty.tipAi": "AI 작성으로 초안을 받아 보강",
  "notes.empty.tipMermaid": "mermaid 다이어그램 렌더 · 확대",

  // 새 노트/새 폴더/이름 변경 모달
  "notes.working": "처리 중…",
  "notes.create": "만들기",
  "notes.renameConfirm": "변경",
  "notes.field.location": "위치",
  "notes.field.noteName": "노트 이름",
  "notes.field.folderName": "폴더 이름",
  "notes.ph.noteName": "예: TCP 혼잡 제어  ·  네트워크/TCP",
  "notes.ph.folderName": "예: 네트워크  ·  CS/네트워크",
  "notes.hint.path": "/ 로 구분하면 중간 폴더가 자동으로 만들어져요.",

  // 삭제 확인 모달 — "{name}" 자리에 <b>이름</b>이 들어간다 (호출부에서 split)
  "notes.delete.noteTitle": "노트 삭제",
  "notes.delete.folderTitle": "폴더 삭제",
  "notes.delete.confirmNote": "{name} 노트를 삭제할까요?",
  "notes.delete.confirmFolder": "{name} 폴더와 안의 모든 노트를 삭제할까요?",
  "notes.delete.trashHint": "휴지통으로 옮겨져요 — Finder 에서 되돌릴 수 있어요.",
  "notes.deleting": "삭제 중…",

  // 저장 안 된 변경 모달
  "notes.unsaved.title": "저장하지 않은 변경",
  "notes.unsaved.body": "지금 노트에 저장하지 않은 변경이 있어요. 버리고 이동할까요?",
  "notes.unsaved.discard": "버리고 이동",
  "notes.keepEditing": "계속 편집",

  // 외부 수정(mtime) 충돌 모달
  "notes.conflict.title": "파일이 밖에서 바뀌었어요",
  "notes.conflict.body":
    "이 노트를 연 뒤에 다른 프로그램(Obsidian·vim·git 등)이 파일을 고쳤어요",
  "notes.conflict.diskTime": "디스크 수정 {time}",
  "notes.conflict.warn": "덮어쓰면 그 변경이 사라져요.",
  "notes.conflict.reload": "내 편집 버리고 다시 읽기",
  "notes.conflict.overwrite": "그래도 덮어쓰기",

  // 인라인 질문(코멘트) 레이어
  "notes.cmt.ask": "질문",
  "notes.cmt.askTip": "선택한 부분에 질문 달기",
  "notes.cmt.promote": "개념으로",
  "notes.cmt.promoteTip": "선택한 부분을 개념 카드로 만들기",
  "notes.cmt.askPh": "이 부분에서 무엇이 궁금한가요?",
  "notes.cmt.askAi": "AI에게 질문",
  "notes.cmt.thinking": "답변 생성 중…",
  "notes.cmt.followUpPh": "이어서 질문하기…",
  "notes.cmt.followUpSend": "후속 질문 보내기",
  "notes.cmt.deleteThread": "질문 스레드 삭제",

  // AI 작성 모달
  "notes.ai.title": "AI로 노트 작성",
  "notes.ai.run": "AI로 작성",
  "notes.ai.configLoading": "설정을 불러오는 중이에요",
  "notes.ai.back": "다시 지시",
  "notes.ai.applyDiff": "변경 적용",
  "notes.ai.applyNew": "에디터에 적용",
  "notes.ai.instructionLabel": "무엇을 써 드릴까요? — Claude에게 지시",
  "notes.ai.instructionPh":
    "예: Rust 변수와 가변성(mut, shadowing)을 예제 코드와 함께 정리해줘 · 지금 노트에 소유권과의 관계 섹션을 추가해줘…",
  "notes.ai.hint":
    "현재 노트가 비어 있으면 처음부터 작성하고, 내용이 있으면 문체·구조를 보존하며 보강해요. 결과는 에디터 초안으로 들어가니 확인 후 ⌘S로 저장하세요.",
  "notes.ai.savedPrompts": "내 프롬프트",
  "notes.ai.presets": "빠른 지시",
  "notes.ai.preset1": "이 주제로 처음부터 정리",
  "notes.ai.preset2": "구체적인 예시·코드 추가",
  "notes.ai.preset3": "더 깊고 자세하게",
  "notes.ai.preset4": "핵심만 간결하게 압축",
  "notes.ai.preset5": "표로 정리",
  "notes.ai.writing": "Claude가 작성하는 중…",
  "notes.ai.waiting": "응답을 기다리는 중…",
  "notes.ai.resultEdited": "AI 편집 결과",
  "notes.ai.resultNew": "작성 결과",
  "notes.ai.tabDiff": "변경사항",
  "notes.ai.tabPreview": "미리보기",
  "notes.ai.tabSource": "소스",
  // "{apply}" 자리에 <b>변경 적용</b> 버튼 라벨이 들어간다 (호출부에서 split)
  "notes.ai.diffHint":
    "현재 노트와 비교한 변경점이에요. {apply}을 누르면 결과가 에디터 초안으로 들어가고, 저장(⌘S) 전까지 파일은 그대로예요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "notes.title": "Notes",

  "notes.newNote": "New note",
  "notes.newFolder": "New folder",
  "notes.refresh": "Refresh",
  "notes.tooltip.newNote": "New note · {dir}",
  "notes.tooltip.newFolder": "New folder · {dir}",
  "notes.tooltip.refresh": "Refresh · pick up Finder changes",

  "notes.row.newNoteHere": "New note in this folder",
  "notes.row.newFolderHere": "New subfolder",
  "notes.rename": "Rename",

  "notes.tree.empty.lead": "No notes yet.",
  "notes.tree.empty.sub": "Sort into folders and write in Markdown.",
  "notes.tree.firstNote": "Create your first note",

  "notes.readError": "Can't read this note — {error}",
  "notes.saveCmd": "Save (⌘S)",
  "notes.saving": "Saving…",
  "notes.edit": "Edit",
  "notes.aiWrite": "Write with AI",
  "notes.ai.fromDraftTip": "AI writes or builds on the current draft",
  "notes.ai.fromNoteTip": "AI writes or builds on the current note",
  "notes.toc": "Contents",
  "notes.madeConcepts": "Concepts made from this note",
  "notes.openConcept": "Open concept — “{anchor}”",
  "notes.meta.modified": "edited {time}",
  "notes.meta.qcount.one": "1 question",
  "notes.meta.qcount.other": "{n} questions",

  "notes.empty.lead": "Open a note on the left, or create a new one to start writing.",
  "notes.empty.sub": "Sort into folders and write freely in Markdown.",
  "notes.empty.tipAi": "Get an AI draft, then build on it",
  "notes.empty.tipMermaid": "mermaid diagram rendering · zoom",

  "notes.working": "Working…",
  "notes.create": "Create",
  "notes.renameConfirm": "Rename",
  "notes.field.location": "Location",
  "notes.field.noteName": "Note name",
  "notes.field.folderName": "Folder name",
  "notes.ph.noteName": "e.g. TCP congestion control  ·  Networking/TCP",
  "notes.ph.folderName": "e.g. Networking  ·  CS/Networking",
  "notes.hint.path": "Separate with / to auto-create intermediate folders.",

  "notes.delete.noteTitle": "Delete note",
  "notes.delete.folderTitle": "Delete folder",
  "notes.delete.confirmNote": "Delete the note {name}?",
  "notes.delete.confirmFolder": "Delete the folder {name} and all notes inside?",
  "notes.delete.trashHint": "It moves to the Trash — you can restore it in Finder.",
  "notes.deleting": "Deleting…",

  "notes.unsaved.title": "Unsaved changes",
  "notes.unsaved.body": "This note has unsaved changes. Discard them and switch?",
  "notes.unsaved.discard": "Discard and switch",
  "notes.keepEditing": "Keep editing",

  "notes.conflict.title": "File changed on disk",
  "notes.conflict.body":
    "Another program (Obsidian, vim, git, …) changed this file after you opened it",
  "notes.conflict.diskTime": "disk edited {time}",
  "notes.conflict.warn": "Overwriting will discard those changes.",
  "notes.conflict.reload": "Discard my edits and reload",
  "notes.conflict.overwrite": "Overwrite anyway",

  "notes.cmt.ask": "Ask",
  "notes.cmt.askTip": "Ask a question about the selection",
  "notes.cmt.promote": "Make concept",
  "notes.cmt.promoteTip": "Turn the selection into a concept card",
  "notes.cmt.askPh": "What are you curious about here?",
  "notes.cmt.askAi": "Ask AI",
  "notes.cmt.thinking": "Writing an answer…",
  "notes.cmt.followUpPh": "Ask a follow-up…",
  "notes.cmt.followUpSend": "Send follow-up",
  "notes.cmt.deleteThread": "Delete this thread",

  "notes.ai.title": "Write note with AI",
  "notes.ai.run": "Write with AI",
  "notes.ai.configLoading": "Loading settings…",
  "notes.ai.back": "Revise instructions",
  "notes.ai.applyDiff": "Apply changes",
  "notes.ai.applyNew": "Apply to editor",
  "notes.ai.instructionLabel": "What should Claude write? — your instructions",
  "notes.ai.instructionPh":
    "e.g. Cover Rust variables and mutability (mut, shadowing) with example code · Add a section on how this relates to ownership…",
  "notes.ai.hint":
    "An empty note is written from scratch; existing content is expanded while keeping its style and structure. The result becomes an editor draft — review it, then save with ⌘S.",
  "notes.ai.savedPrompts": "My prompts",
  "notes.ai.presets": "Quick instructions",
  "notes.ai.preset1": "Write this topic from scratch",
  "notes.ai.preset2": "Add concrete examples and code",
  "notes.ai.preset3": "Go deeper and more detailed",
  "notes.ai.preset4": "Condense to the essentials",
  "notes.ai.preset5": "Organize as a table",
  "notes.ai.writing": "Claude is writing…",
  "notes.ai.waiting": "Waiting for a response…",
  "notes.ai.resultEdited": "AI edit result",
  "notes.ai.resultNew": "Result",
  "notes.ai.tabDiff": "Changes",
  "notes.ai.tabPreview": "Preview",
  "notes.ai.tabSource": "Source",
  "notes.ai.diffHint":
    "Changes compared to the current note. Press {apply} to load the result into the editor as a draft — the file stays as is until you save (⌘S).",
} as const;

export const notesMessages = { ko, en };
