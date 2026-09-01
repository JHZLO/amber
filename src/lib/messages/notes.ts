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
  "notes.copy": "복사",
  "notes.copied": "복사됨",
  "notes.copyAll": "노트 전문을 마크다운 원문으로 클립보드에 복사해요",
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
  "notes.cmt.edit": "수정",
  "notes.cmt.editTip": "이 부분을 AI로 고쳐 쓰기 (드래그가 걸친 문단·표 단위)",
  "notes.cmt.askPh": "이 부분에서 무엇이 궁금한가요?",
  "notes.cmt.askAi": "AI에게 질문",
  "notes.cmt.thinking": "답변 생성 중…",
  "notes.cmt.revise": "고쳐 쓰기",
  "notes.cmt.revisePh": "어떻게 고칠까요? 예: 한국어로 바꿔줘 · 더 짧게 · 예시 하나 추가",
  "notes.cmt.reviseSend": "이 답변을 지시대로 고쳐 씁니다 (기존 답변을 교체해요)",
  "notes.cmt.revising": "답변을 고쳐 쓰는 중…",
  "notes.cmt.followUpPh": "이어서 질문하기…",
  "notes.cmt.followUpSend": "후속 질문 보내기",
  "notes.cmt.deleteThread": "질문 스레드 삭제",

  // 글 단위 질문 목록 — 문장을 클릭하는 길과 별도로, 이 노트의 질문을 모아 본다
  "notes.qlist.btn": "질문 {n}",
  "notes.qlist.title": "이 노트의 질문 {n}개",
  "notes.qlist.empty": "아직 단 질문이 없어요.",
  "notes.qlist.turns": "문답 {n}",
  "notes.qlist.back": "목록으로",
  // 앵커 문장이 수정돼 본문에서 못 찾은 스레드 — 목록으로 계속 닿으니 경고가 아니라 표시만
  "notes.qlist.missing": "본문에 없음",
  "notes.qlist.missingTip":
    "질문을 단 문장이 수정돼 본문에서 찾지 못했어요. 문답은 그대로 남아 있어요.",

  // 부분만 고쳐 쓰는 AI 모달 (전문 재작성과 별개 — 출력이 조각뿐이라 훨씬 빠르다)
  "notes.spanAi.title": "부분만 AI로 수정",
  "notes.spanAi.selBtn": "선택 수정",
  "notes.spanAi.selBtnTip": "선택한 부분만 AI로 고쳐 써요 — 전문을 다시 받지 않아 훨씬 빨라요",
  "notes.spanAi.selBtnEmpty": "왼쪽 원문에서 고칠 부분을 먼저 선택해 주세요",
  "notes.spanAi.secBtn": "절 수정",
  "notes.spanAi.secBtnTip": "절 하나만 골라 AI로 고쳐 써요",
  "notes.spanAi.pickLabel": "고칠 절",
  "notes.spanAi.backToPick": "다시 고르기",
  "notes.spanAi.pickCount": "{n}개 선택 · {chars}자",
  "notes.spanAi.next": "다음",
  "notes.spanAi.runsLabel": "고칠 {n}묶음",
  "notes.spanAi.runsHint":
    "떨어져 있는 묶음이라 묶음마다 따로 고쳐 써요. 이어진 절은 한 묶음으로 묶여 한 번에 고쳐요.",
  "notes.spanAi.progress": "{i}/{n} 묶음 고쳐 쓰는 중…",
  "notes.spanAi.pickHint": "고른 절만 다시 써요. 여러 개 골라도 되고, 이어진 절은 한 번에 고쳐요.",
  "notes.spanAi.noSections":
    "제목이 없어서 나눌 절이 없어요. 편집 모드에서 고칠 부분을 선택해 수정해 보세요.",
  "notes.spanAi.selectionLabel": "고칠 부분 (선택 영역)",
  "notes.spanAi.chars": "{n}자",
  "notes.spanAi.instructionLabel": "어떻게 고칠까요?",
  "notes.spanAi.instructionPh": "예: 이 문단을 짧게 줄여줘 / 표로 바꿔줘 / 예시 하나 추가",
  "notes.spanAi.hint": "이 부분만 바뀌어요. 나머지 본문은 그대로 남아요. (⌘Enter 로 실행)",
  "notes.spanAi.run": "이 부분만 수정",
  "notes.spanAi.editing": "이 부분을 고쳐 쓰는 중…",
  "notes.spanAi.resultLabel": "고쳐 쓴 부분",
  "notes.spanAi.apply": "이 부분 반영",
  "notes.spanAi.applyHint": "반영하면 편집 초안의 그 자리에만 들어가요. 저장은 ⌘S 로 해요.",

  // AI 작성 모달
  "notes.ai.title": "AI로 노트 작성",
  "notes.ai.run": "AI로 작성",
  "notes.ai.stop": "중단",
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
  "notes.copy": "Copy",
  "notes.copied": "Copied",
  "notes.copyAll": "Copy the whole note to the clipboard as markdown",
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
  "notes.cmt.edit": "Edit",
  "notes.cmt.editTip": "Rewrite this part with AI (the paragraph or table the drag touches)",
  "notes.cmt.askPh": "What are you curious about here?",
  "notes.cmt.askAi": "Ask AI",
  "notes.cmt.thinking": "Writing an answer…",
  "notes.cmt.revise": "Rewrite",
  "notes.cmt.revisePh": "How should it change? e.g. in Korean · shorter · add an example",
  "notes.cmt.reviseSend": "Rewrite this answer as instructed (it replaces the old one)",
  "notes.cmt.revising": "Rewriting the answer…",
  "notes.cmt.followUpPh": "Ask a follow-up…",
  "notes.cmt.followUpSend": "Send follow-up",
  "notes.cmt.deleteThread": "Delete this thread",

  "notes.qlist.btn": "{n} questions",
  "notes.qlist.title": "{n} questions in this note",
  "notes.qlist.empty": "No questions here yet.",
  "notes.qlist.turns": "{n} turns",
  "notes.qlist.back": "Back to the list",
  "notes.qlist.missing": "not in the text",
  "notes.qlist.missingTip":
    "The sentence this was attached to has changed, so it is no longer in the text. The thread is still here.",

  "notes.spanAi.title": "Edit one part with AI",
  "notes.spanAi.selBtn": "Edit selection",
  "notes.spanAi.selBtnTip": "Rewrite just the selected part — the whole note is never re-emitted, so it is far faster",
  "notes.spanAi.selBtnEmpty": "Select the part to edit in the source pane first",
  "notes.spanAi.secBtn": "Edit section",
  "notes.spanAi.secBtnTip": "Pick one section and rewrite just that",
  "notes.spanAi.pickLabel": "Section to edit",
  "notes.spanAi.backToPick": "Pick again",
  "notes.spanAi.pickCount": "{n} picked · {chars} chars",
  "notes.spanAi.next": "Next",
  "notes.spanAi.runsLabel": "{n} stretches to edit",
  "notes.spanAi.runsHint":
    "These picks are not adjacent, so each stretch is rewritten on its own. Sections that touch are merged into one.",
  "notes.spanAi.progress": "Rewriting stretch {i} of {n}…",
  "notes.spanAi.pickHint": "Only what you pick is rewritten. Pick several if you like — sections that touch are done in one pass.",
  "notes.spanAi.noSections":
    "No headings to split on. Select the part to edit in edit mode instead.",
  "notes.spanAi.selectionLabel": "Part to edit (selection)",
  "notes.spanAi.chars": "{n} chars",
  "notes.spanAi.instructionLabel": "How should it change?",
  "notes.spanAi.instructionPh": "e.g. shorten this paragraph / turn it into a table / add one example",
  "notes.spanAi.hint": "Only this part changes; the rest of the note stays as it is. (⌘Enter to run)",
  "notes.spanAi.run": "Edit this part",
  "notes.spanAi.editing": "Rewriting this part…",
  "notes.spanAi.resultLabel": "Rewritten part",
  "notes.spanAi.apply": "Apply to this part",
  "notes.spanAi.applyHint": "Applying puts it back in place in the draft. Save with ⌘S.",

  "notes.ai.title": "Write note with AI",
  "notes.ai.run": "Write with AI",
  "notes.ai.stop": "Stop",
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
