// 공용 문자열 — 공유 컴포넌트(ui.tsx)와 공유 lib(vaultTree·ai·date)가 쓰는 것만.
// 특정 화면 전용 문자열은 그 도메인 파일에 둔다.

const ko = {
  "common.close": "닫기",
  "common.cancel": "취소",
  "common.save": "저장",
  "common.delete": "삭제",

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

  "common.ai.notFound": "{cli} CLI를 찾을 수 없어요. 설정(⚙)에서 경로를 확인하세요.",
  "common.ai.auth": "{cli} 인증이 필요해요. 터미널에서 `{cli}` 로그인 후 다시 시도하세요.",
  "common.ai.rateLimit": "사용량 한도에 도달했어요. 잠시 후 다시 시도하세요.",
  "common.ai.timeout": "응답이 너무 오래 걸렸어요. 다시 시도해 주세요.",
  "common.ai.badResult": "정리 결과를 해석하지 못했어요. 다시 생성하거나 수동으로 작성하세요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "common.close": "Close",
  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.delete": "Delete",

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

  "common.ai.notFound": "Couldn't find the {cli} CLI. Check its path in Settings (⚙).",
  "common.ai.auth": "{cli} needs authentication. Log in with `{cli}` in a terminal, then retry.",
  "common.ai.rateLimit": "You've hit the usage limit. Try again in a bit.",
  "common.ai.timeout": "The response took too long. Please try again.",
  "common.ai.badResult": "Couldn't parse the result. Regenerate or write it manually.",
} as const;

export const commonMessages = { ko, en };
