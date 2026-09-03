// settings 도메인 문자열 — 키는 "settings." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).
// 제품명(Claude Code · OpenAI Codex CLI · Gemini CLI)과 모델명(Opus 5 등)은 번역하지 않는다 —
// 모델 라벨은 config.ts 에서 "모델명 (수식어)" 로 조합하고 여기엔 수식어만 둔다.
// 언어 선택지 라벨(한국어/English)은 자기 표기 그대로라 사전에 넣지 않는다 (SettingsModal 의 LANGS).

const ko = {
  // 설정 모달 공통
  "settings.title": "설정",
  "settings.tab.ai": "AI",
  "settings.tab.prompts": "프롬프트",
  "settings.tab.report": "데일리 리포트",
  "settings.tab.appearance": "모양",
  "settings.tab.databases": "데이터베이스",
  "settings.openDataFolder": "데이터 폴더 열기",
  "settings.openFolderFail": "폴더 열기 실패: {err}",

  // 백업
  "settings.backup": "백업",
  "settings.backup.busy": "백업 중…",
  "settings.backup.pickTitle": "백업을 저장할 폴더 선택",
  "settings.backup.done": "백업 완료 — {path}",
  "settings.backup.fail": "백업 실패 — {err}",

  // AI 연결 (설정 탭 + 온보딩 공용)
  "settings.ai.title": "AI 연결",
  "settings.ai.redetect": "다시 감지",
  "settings.ai.detecting": "감지 중…",
  "settings.ai.connected":
    "현재 {name} 에 연결돼 있어요. 로컬 CLI 의 로그인 세션을 그대로 사용합니다.",
  "settings.ai.none": "연결된 AI 가 없어요. 설치된 CLI 를 감지해 연결하세요.",
  "settings.ai.searching": "설치된 AI CLI 를 찾는 중…",
  "settings.ai.notFound":
    "설치된 AI CLI 를 찾지 못했어요. claude · codex · gemini 중 하나를 설치하고 로그인한 뒤 다시 감지하세요.",
  "settings.ai.pathLabel": "{name} 경로",
  "settings.ai.test": "연결 테스트",
  "settings.ai.testOk": "연결 성공 — {version}",
  "settings.ai.modelLabel": "모델",
  "settings.ai.langLabel": "응답 언어",
  "settings.ai.langAuto": "자동 (화면 언어를 따름)",
  "settings.ai.langHint":
    "AI 답변·노트·리포트를 이 언어로만 씁니다. 노트가 영어로 쓰여 있어도 이 설정을 따라요 (코드·식별자는 원문 유지).",
  "settings.ai.creditHint": "AI 호출은 연결된 CLI 의 플랜/크레딧을 소모해요.",

  // CLI 로그인 (AiAuthModal + 설정 › AI 의 인증 줄)
  "settings.auth.title": "AI 로그인",
  "settings.auth.checking": "인증 상태를 확인하는 중…",
  "settings.auth.lead":
    "{name} 인증이 만료됐어요. 여기서 다시 로그인하면 AI 기능이 바로 살아나요.",
  "settings.auth.alreadyOk": "{name} 에 이미 로그인돼 있어요.",
  "settings.auth.unsupported":
    "{name} 은 앱 안에서 로그인할 수 없어요. 터미널에서 로그인한 뒤 다시 시도하세요.",
  "settings.auth.start": "로그인 시작",
  "settings.auth.again": "다시 로그인",
  "settings.auth.waiting": "브라우저에서 로그인을 기다리는 중…",
  "settings.auth.urlLabel": "인증 주소",
  "settings.auth.openBrowser": "브라우저에서 열기",
  "settings.auth.urlHint": "브라우저가 저절로 열리지 않았다면 이 주소를 여세요.",
  "settings.auth.codeLabel": "인증 코드",
  "settings.auth.codePlaceholder": "브라우저에서 받은 코드를 붙여넣으세요",
  "settings.auth.codeSafety": "코드는 CLI 로 그대로 전달되고 Amber 에는 저장되지 않아요.",
  "settings.auth.submit": "코드 전달",
  "settings.auth.done": "로그인됐어요. 하던 작업을 다시 시도하면 돼요.",
  "settings.auth.okNote": "{name} 로그인 완료",
  "settings.auth.failed": "로그인이 끝나지 않았어요. 다시 시도해 주세요.",
  // 설정 › AI 의 인증 줄
  "settings.auth.rowOk": "로그인됨",
  "settings.auth.rowExpired": "만료됨 — 다시 로그인이 필요해요",
  "settings.auth.rowUnknown": "확인할 수 없음",
  "settings.auth.rowAction": "로그인",

  // 모델 라벨 수식어 (config.ts 의 PROVIDER_MODELS 에서 조합)
  "settings.model.latestQuality": "최신·품질 우선",
  "settings.model.quality": "품질 우선",
  "settings.model.balanced": "균형",
  "settings.model.fast": "속도·비용 절약",
  "settings.model.latest": "최신",
  "settings.model.cliDefault": "CLI 기본 모델",

  // 저장 프롬프트
  "settings.prompts.title": "저장 프롬프트",
  "settings.prompts.desc.pre": "자주 쓰는 지시를 저장해 두면 ",
  "settings.prompts.desc.bold": "AI로 노트 작성",
  "settings.prompts.desc.post": " 모달에서 체크 한 번으로 요청에 함께 보낼 수 있어요.",
  "settings.prompts.empty": "저장된 프롬프트가 없어요. “새 프롬프트”로 추가하세요.",
  "settings.prompt.new": "새 프롬프트",
  "settings.prompt.editTitle": "프롬프트 편집",
  "settings.prompt.nameLabel": "이름",
  "settings.prompt.namePlaceholder": "예: 개념노트 보강",
  "settings.prompt.textLabel": "프롬프트 (Claude에게 줄 지시)",
  "settings.prompt.textPlaceholder":
    "예: 진짜 개념노트처럼 대/중/소제목으로 분류하고 예시 코드와 표를 넣어 상세히 보강해줘",
  "settings.prompt.nameHint": "이름을 비우면 지시문 앞부분이 이름으로 쓰여요.",

  // 모양 — 테마·언어
  "settings.theme.label": "테마",
  "settings.theme.system": "시스템 설정 따름",
  "settings.theme.light": "라이트",
  "settings.theme.dark": "다크",
  "settings.lang.label": "언어",
  "settings.lang.confirmTitle": "언어 변경",
  "settings.lang.confirmBody":
    "언어를 바꾸면 화면을 다시 불러와요. 저장하지 않은 편집이 있다면 먼저 저장하세요.",
  "settings.lang.apply": "바꾸기",

  // AI 온보딩 (최초 1회)
  "settings.onb.skip": "AI 없이 사용",
  "settings.onb.connect": "연결",
  "settings.onb.connecting": "연결 중…",
  "settings.onb.lead.pre": "Amber 의 AI 기능(개념 정리·노트 작성·인라인 질문)은 ",
  "settings.onb.lead.bold": "이미 쓰고 있는 AI CLI 의 로그인 세션",
  "settings.onb.lead.post": "을 그대로 사용해요. API 키를 따로 저장하지 않습니다.",
  "settings.onb.searchHint": "로그인 셸 PATH 에서 claude · codex · gemini 를 확인해요",
  "settings.onb.emptyTitle": "설치된 AI CLI 를 찾지 못했어요.",
  "settings.onb.emptyBody":
    "아래 중 하나를 설치·로그인한 뒤 설정(⚙)에서 다시 감지할 수 있어요.",
  "settings.onb.installGuide": "{name} 설치 안내",
  "settings.onb.laterHint": "나중에 설정(⚙)에서 언제든 다시 연결하거나 바꿀 수 있어요.",

  // 워크스페이스 루트 전환기 (RootPicker)
  "settings.root.default": "기본 보관함",
  "settings.root.defaultTitle": "기본 보관함 (앱 데이터 폴더)",
  "settings.root.appData": "앱 데이터 폴더",
  "settings.root.openFolder": "폴더 열기…",
  "settings.root.openFolderDesc": "로컬 폴더를 워크스페이스로 사용",
  "settings.root.openDialogTitle": "작업 폴더 열기",
  "settings.root.copyPath": "경로 복사",
  "settings.root.copied": "복사됨",

  // 데이터베이스 연결 관리 (DB 스키마 연동)
  "settings.db.title": "데이터베이스 연결",
  "settings.db.add": "연결 추가",
  "settings.db.none":
    "연결된 데이터베이스가 없어요. 연결하면 스키마별 ERD 가 다이어그램 탭에 자동으로 생겨요.",
  "settings.db.schemas": "스키마 {n}",
  "settings.db.folder": "폴더 {path}",
  "settings.db.edit": "편집",
  "settings.db.enterPassword": "비밀번호 입력",
  "settings.db.deleteTitle": "연결 삭제",
  "settings.db.deleteBody": "연결과 저장된 비밀번호를 지울까요? 다이어그램 파일은 그대로 남아요.",
  "settings.db.hint":
    "비밀번호는 macOS 키체인에만 저장돼요. 연결을 삭제하면 키체인 항목도 함께 지워지고, 생성된 다이어그램 파일은 남아요 — 폴더는 일반 폴더로 돌아가요.",
  "settings.db.passwordTitle": "비밀번호 입력 — {name}",
  "settings.db.passwordSaved": "저장했어요. 다음 동기화부터 이 비밀번호를 써요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "settings.title": "Settings",
  "settings.tab.ai": "AI",
  "settings.tab.prompts": "Prompts",
  "settings.tab.report": "Daily report",
  "settings.tab.appearance": "Appearance",
  "settings.tab.databases": "Databases",
  "settings.openDataFolder": "Open data folder",
  "settings.openFolderFail": "Couldn't open the folder: {err}",

  "settings.backup": "Back up",
  "settings.backup.busy": "Backing up…",
  "settings.backup.pickTitle": "Choose a folder for the backup",
  "settings.backup.done": "Backup complete — {path}",
  "settings.backup.fail": "Backup failed — {err}",

  "settings.ai.title": "AI connection",
  "settings.ai.redetect": "Detect again",
  "settings.ai.detecting": "Detecting…",
  "settings.ai.connected":
    "Connected to {name}. Amber reuses your local CLI's login session.",
  "settings.ai.none": "No AI connected. Detect an installed CLI to connect one.",
  "settings.ai.searching": "Looking for installed AI CLIs…",
  "settings.ai.notFound":
    "Couldn't find an installed AI CLI. Install and log in to one of claude · codex · gemini, then detect again.",
  "settings.ai.pathLabel": "{name} path",
  "settings.ai.test": "Test connection",
  "settings.ai.testOk": "Connected — {version}",
  "settings.ai.modelLabel": "Model",
  "settings.ai.langLabel": "Response language",
  "settings.ai.langAuto": "Auto (follow the interface)",
  "settings.ai.langHint":
    "AI answers, notes and reports are written only in this language, even when the note itself is in another one (code and identifiers stay as-is).",
  "settings.ai.creditHint": "AI calls use the connected CLI's plan/credits.",

  // CLI sign-in (AiAuthModal + the auth row in Settings › AI)
  "settings.auth.title": "AI sign-in",
  "settings.auth.checking": "Checking sign-in status…",
  "settings.auth.lead":
    "Your {name} sign-in has expired. Sign in here and the AI features come right back.",
  "settings.auth.alreadyOk": "You're already signed in to {name}.",
  "settings.auth.unsupported":
    "{name} can't be signed in from the app. Log in from a terminal, then retry.",
  "settings.auth.start": "Start sign-in",
  "settings.auth.again": "Sign in again",
  "settings.auth.waiting": "Waiting for the browser sign-in…",
  "settings.auth.urlLabel": "Sign-in address",
  "settings.auth.openBrowser": "Open in browser",
  "settings.auth.urlHint": "If the browser didn't open on its own, open this address.",
  "settings.auth.codeLabel": "Sign-in code",
  "settings.auth.codePlaceholder": "Paste the code from the browser",
  "settings.auth.codeSafety": "The code goes straight to the CLI — Amber never stores it.",
  "settings.auth.submit": "Send code",
  "settings.auth.done": "Signed in. You can retry what you were doing.",
  "settings.auth.okNote": "Signed in to {name}",
  "settings.auth.failed": "The sign-in didn't complete. Please try again.",
  // The auth row in Settings › AI
  "settings.auth.rowOk": "Signed in",
  "settings.auth.rowExpired": "Expired — sign in again",
  "settings.auth.rowUnknown": "Can't tell",
  "settings.auth.rowAction": "Sign in",

  "settings.model.latestQuality": "latest · best quality",
  "settings.model.quality": "best quality",
  "settings.model.balanced": "balanced",
  "settings.model.fast": "fast · lower cost",
  "settings.model.latest": "latest",
  "settings.model.cliDefault": "CLI default model",

  "settings.prompts.title": "Saved prompts",
  "settings.prompts.desc.pre": "Save the instructions you use often and tick them in the ",
  "settings.prompts.desc.bold": "Write note with AI",
  "settings.prompts.desc.post": " modal — they travel with your request instead of filling the box.",
  "settings.prompts.empty": "No saved prompts yet. Add one with “New prompt”.",
  "settings.prompt.new": "New prompt",
  "settings.prompt.editTitle": "Edit prompt",
  "settings.prompt.nameLabel": "Name",
  "settings.prompt.namePlaceholder": "e.g. Expand concept note",
  "settings.prompt.textLabel": "Prompt (instructions for Claude)",
  "settings.prompt.textPlaceholder":
    "e.g. Structure it like a real concept note with nested headings, and enrich it with example code and tables",
  "settings.prompt.nameHint": "Leave the name empty to use the start of the instruction.",

  "settings.theme.label": "Theme",
  "settings.theme.system": "Follow system setting",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",
  "settings.lang.label": "Language",
  "settings.lang.confirmTitle": "Change language",
  "settings.lang.confirmBody":
    "Changing the language reloads the window. If you have unsaved edits, save them first.",
  "settings.lang.apply": "Change",

  "settings.onb.skip": "Use without AI",
  "settings.onb.connect": "Connect",
  "settings.onb.connecting": "Connecting…",
  "settings.onb.lead.pre":
    "Amber's AI features (concept summaries, note writing, inline questions) reuse ",
  "settings.onb.lead.bold": "the login session of the AI CLI you already use",
  "settings.onb.lead.post": ". No API key is stored.",
  "settings.onb.searchHint": "Checks your login shell PATH for claude · codex · gemini",
  "settings.onb.emptyTitle": "Couldn't find an installed AI CLI.",
  "settings.onb.emptyBody":
    "Install and log in to one below, then detect again in Settings (⚙).",
  "settings.onb.installGuide": "{name} install guide",
  "settings.onb.laterHint": "You can reconnect or switch anytime in Settings (⚙).",

  "settings.root.default": "Default vault",
  "settings.root.defaultTitle": "Default vault (app data folder)",
  "settings.root.appData": "App data folder",
  "settings.root.openFolder": "Open folder…",
  "settings.root.openFolderDesc": "Use a local folder as the workspace",
  "settings.root.openDialogTitle": "Open a working folder",
  "settings.root.copyPath": "Copy path",
  "settings.root.copied": "Copied",

  "settings.db.title": "Database connections",
  "settings.db.add": "Add connection",
  "settings.db.none":
    "No databases connected. Connect one and an ERD per schema appears in the Diagrams tab automatically.",
  "settings.db.schemas": "{n} schemas",
  "settings.db.folder": "folder {path}",
  "settings.db.edit": "Edit",
  "settings.db.enterPassword": "Enter password",
  "settings.db.deleteTitle": "Delete connection",
  "settings.db.deleteBody": "Delete the connection and its saved password? Diagram files stay where they are.",
  "settings.db.hint":
    "Passwords live only in the macOS Keychain. Deleting a connection removes its Keychain item too; generated diagram files stay and the folder becomes a plain folder.",
  "settings.db.passwordTitle": "Enter password — {name}",
  "settings.db.passwordSaved": "Saved. The next sync uses this password.",
} as const;

export const settingsMessages = { ko, en };
