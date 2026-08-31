// 공용 문자열 — 공유 컴포넌트(ui.tsx)와 공유 lib(vaultTree·ai·date)가 쓰는 것만.
// 특정 화면 전용 문자열은 그 도메인 파일에 둔다.

const ko = {
  "common.close": "닫기",
  "common.cancel": "취소",
  "common.done": "완료",
  "common.save": "저장",
  "common.delete": "삭제",
  "common.resizePane": "드래그해서 좌측 너비 조절",

  // 미저장 초안 확인 (ui.tsx UnsavedModal) — 노트·다이어그램·개념·작업폴더 전환 공용
  "common.unsaved.title": "저장하지 않은 변경",
  "common.unsaved.body": "저장하지 않은 변경이 있어요. 버리고 이동할까요?",
  "common.unsaved.keep": "계속 편집",
  "common.unsaved.discard": "버리고 이동",

  // 렌더 중 예외 — ErrorBoundary(main.tsx). 창이 백지가 되는 대신 이 화면이 뜬다
  "common.crash.title": "화면을 그리다 문제가 생겼어요",
  "common.crash.hint": "저장된 노트·다이어그램 파일은 그대로예요. 새로고침하면 대부분 복구돼요.",
  "common.crash.reload": "새로고침",

  "common.status.learning": "학습중",
  "common.status.learned": "학습완료",
  "common.confidence": "자신감 {n}/3",

  "common.timeago.now": "방금",
  "common.timeago.minutes": "{m}분 전",
  "common.timeago.hours": "{h}시간 전",
  "common.timeago.days": "{d}일 전",

  "common.name.empty": "이름을 입력하세요.",
  "common.name.badChars": "이름에 / \\ : 는 쓸 수 없어요.",
  "common.name.leadingDot": "이름은 . 으로 시작할 수 없어요.",
  "common.name.tooLong": "이름이 너무 길어요 (80자 이내).",
  "common.file.dupName": "같은 이름이 이미 있어요.",
  "common.file.dupFile": "같은 이름의 파일이 이미 있어요.",
  "common.folder.intoSelf": "폴더를 자기 자신 안으로 옮길 수 없어요.",
  "common.folder.dupTarget": "대상 폴더에 같은 이름이 이미 있어요.",

  // Rust 커맨드 에러 (lib/errors.ts 가 code → 이 키로 변환). {detail} = 가변부(경로·초·OS 메시지)
  "common.err.ai.notFound": "AI CLI 를 찾을 수 없어요: {detail}. 설정(⚙)에서 경로를 확인하세요.",
  "common.err.ai.auth": "AI CLI 인증이 만료됐어요. 로그인 창에서 다시 로그인하세요.",
  "common.err.ai.authUnsupported":
    "이 CLI 는 앱 안에서 로그인할 수 없어요. 터미널에서 로그인한 뒤 다시 시도하세요.",
  "common.err.ai.noLoginSession": "진행 중인 로그인이 없어요. 로그인을 다시 시작하세요.",
  "common.err.ai.rateLimit": "사용량 한도에 도달했어요. 잠시 후 다시 시도하세요.",
  "common.err.ai.timeout": "{detail}초 안에 응답이 없었어요. 다시 시도해 주세요.",
  "common.err.ai.badEnvelope":
    "CLI 응답을 해석하지 못했어요. CLI 버전을 확인해 보세요. {detail}",
  "common.err.ai.badResult": "생성 결과가 비어 있어요. 다시 시도해 주세요.",
  "common.err.ai.generic": "AI CLI 가 오류를 반환했어요 — {detail}",
  "common.err.ai.spawn": "AI CLI 를 실행하지 못했어요 — {detail}",
  "common.err.ai.stdin": "AI CLI 에 입력을 전달하지 못했어요 — {detail}",
  "common.err.ai.wait": "AI CLI 종료를 기다리지 못했어요 — {detail}",
  "common.err.empty.transcript": "입력이 너무 짧아요. 대화 원문을 붙여넣어 주세요.",
  "common.err.empty.instruction": "지시를 입력해 주세요.",
  "common.err.empty.noteBody": "보강할 노트 본문이 비어 있어요.",
  "common.err.empty.question": "질문을 입력해 주세요.",
  "common.err.empty.selection": "선택한 문장이 비어 있어요.",
  "common.err.empty.ddl": "스키마 DDL 을 붙여넣어 주세요.",
  "common.err.gh.notFound": "gh CLI 를 찾을 수 없어요.",
  "common.err.gh.auth": "gh 인증이 필요해요. 터미널에서 `gh auth login` 후 다시 시도하세요.",
  "common.err.gh.generic": "gh 호출이 실패했어요 — {detail}",
  // GitHub 활동 피드는 한 페이지(최근 {detail}건)까지만 온다 — 그보다 오래된 날짜는 조회 자체가 불가능
  "common.err.gh.windowTruncated":
    "GitHub 활동 피드가 이 날짜까지 닿지 않아요 (최근 {detail}건까지만 조회돼요).",
  "common.err.report.timeout": "수집이 시간 안에 끝나지 않았어요.",
  "common.err.report.noActivity": "이 날짜엔 정리할 활동이 없어요.",
  "common.err.backup.noDest": "백업할 폴더를 찾을 수 없어요.",
  "common.err.backup.insideAppData": "앱 데이터 폴더 밖의 위치를 선택해 주세요.",
  "common.err.backup.mkdir": "백업 폴더를 만들지 못했어요 — {detail}",
  "common.err.backup.write": "백업을 쓰지 못했어요 — {detail}",
  "common.err.backup.path": "앱 데이터 폴더를 찾지 못했어요.",
  "common.err.trash.forbidden": "허용되지 않은 경로예요.",
  "common.err.trash.failed": "휴지통으로 옮기지 못했어요 — {detail}",

  // 마크다운 알림 블록(`> [!NOTE]`) 라벨 — 모노톤이라 색이 아니라 이 라벨과 아이콘이 종류를 나른다
  "common.alert.note": "참고",
  "common.alert.tip": "팁",
  "common.alert.important": "중요",
  "common.alert.warning": "주의",
  "common.alert.caution": "경고",

  // 페이지 내 검색(⌘F) — 필기노트·개념·투두 공용 (components/PageFind.tsx)
  "common.find.ph": "이 화면에서 찾기",
  "common.find.prev": "이전 결과",
  "common.find.next": "다음 결과",

  "common.ai.notFound": "{cli} CLI를 찾을 수 없어요. 설정(⚙)에서 경로를 확인하세요.",
  "common.ai.auth": "{cli} 인증이 필요해요. 터미널에서 `{cli}` 로그인 후 다시 시도하세요.",
  "common.ai.rateLimit": "사용량 한도에 도달했어요. 잠시 후 다시 시도하세요.",
  "common.ai.timeout": "응답이 너무 오래 걸렸어요. 다시 시도해 주세요.",
  "common.ai.badResult": "정리 결과를 해석하지 못했어요. 다시 생성하거나 수동으로 작성하세요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "common.close": "Close",
  "common.cancel": "Cancel",
  "common.done": "Done",
  "common.save": "Save",
  "common.delete": "Delete",
  "common.resizePane": "Drag to resize the left pane",

  "common.unsaved.title": "Unsaved changes",
  "common.unsaved.body": "There are unsaved changes. Discard them and switch?",
  "common.unsaved.keep": "Keep editing",
  "common.unsaved.discard": "Discard & switch",

  "common.crash.title": "Something broke while rendering",
  "common.crash.hint":
    "Your notes and diagrams on disk are untouched. Reloading usually fixes it.",
  "common.crash.reload": "Reload",

  "common.status.learning": "Learning",
  "common.status.learned": "Learned",
  "common.confidence": "Confidence {n}/3",

  "common.timeago.now": "just now",
  "common.timeago.minutes": "{m}m ago",
  "common.timeago.hours": "{h}h ago",
  "common.timeago.days": "{d}d ago",

  "common.name.empty": "Enter a name.",
  "common.name.badChars": "Names can't contain / \\ or :.",
  "common.name.leadingDot": "Names can't start with a dot.",
  "common.name.tooLong": "That name is too long (80 characters max).",
  "common.file.dupName": "Something with that name already exists.",
  "common.file.dupFile": "A file with that name already exists.",
  "common.folder.intoSelf": "A folder can't be moved into itself.",
  "common.folder.dupTarget": "The destination already has an item with that name.",

  "common.err.ai.notFound": "Couldn't find the AI CLI: {detail}. Check its path in Settings (⚙).",
  "common.err.ai.auth": "The AI CLI sign-in has expired. Sign in again in the window that opened.",
  "common.err.ai.authUnsupported":
    "This CLI can't be signed in from the app. Log in from a terminal, then retry.",
  "common.err.ai.noLoginSession": "No sign-in is in progress. Start it again.",
  "common.err.ai.rateLimit": "You've hit the usage limit. Try again in a bit.",
  "common.err.ai.timeout": "No response within {detail}s. Please try again.",
  "common.err.ai.badEnvelope":
    "Couldn't parse the CLI response. Check your CLI version. {detail}",
  "common.err.ai.badResult": "The generated result was empty. Please try again.",
  "common.err.ai.generic": "The AI CLI returned an error — {detail}",
  "common.err.ai.spawn": "Couldn't start the AI CLI — {detail}",
  "common.err.ai.stdin": "Couldn't send input to the AI CLI — {detail}",
  "common.err.ai.wait": "Couldn't wait for the AI CLI to exit — {detail}",
  "common.err.empty.transcript": "That input is too short. Paste the raw transcript.",
  "common.err.empty.instruction": "Enter an instruction.",
  "common.err.empty.noteBody": "There's no note content to build on.",
  "common.err.empty.question": "Enter a question.",
  "common.err.empty.selection": "The selected text is empty.",
  "common.err.empty.ddl": "Paste the schema DDL.",
  "common.err.gh.notFound": "Couldn't find the gh CLI.",
  "common.err.gh.auth": "gh needs authentication. Run `gh auth login` in a terminal, then retry.",
  "common.err.gh.generic": "The gh call failed — {detail}",
  "common.err.gh.windowTruncated":
    "GitHub's activity feed doesn't reach back to this date (only the latest {detail} events are available).",
  "common.err.report.timeout": "Collection didn't finish in time.",
  "common.err.report.noActivity": "There's no activity to summarize for this date.",
  "common.err.backup.noDest": "Couldn't find the destination folder.",
  "common.err.backup.insideAppData": "Pick a location outside the app data folder.",
  "common.err.backup.mkdir": "Couldn't create the backup folder — {detail}",
  "common.err.backup.write": "Couldn't write the backup — {detail}",
  "common.err.backup.path": "Couldn't locate the app data folder.",
  "common.err.trash.forbidden": "That path isn't allowed.",
  "common.err.trash.failed": "Couldn't move it to the Trash — {detail}",

  "common.alert.note": "Note",
  "common.alert.tip": "Tip",
  "common.alert.important": "Important",
  "common.alert.warning": "Warning",
  "common.alert.caution": "Caution",

  "common.find.ph": "Find on this screen",
  "common.find.prev": "Previous match",
  "common.find.next": "Next match",

  "common.ai.notFound": "Couldn't find the {cli} CLI. Check its path in Settings (⚙).",
  "common.ai.auth": "{cli} needs authentication. Log in with `{cli}` in a terminal, then retry.",
  "common.ai.rateLimit": "You've hit the usage limit. Try again in a bit.",
  "common.ai.timeout": "The response took too long. Please try again.",
  "common.ai.badResult": "Couldn't parse the result. Regenerate or write it manually.",
} as const;

export const commonMessages = { ko, en };
