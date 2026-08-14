// todos 도메인 문자열 — 키는 "todos." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).

const ko = {
  // 상단 내비게이션 (선택 날짜 이동) — todos.today 는 아젠다 '오늘' 배지와 공용
  "todos.today": "오늘",
  "todos.nav.prevDay": "전날",
  "todos.nav.nextDay": "다음날",

  // 빠른 추가 + 목록
  "todos.quick.placeholder": "할 일을 적고 Enter",
  "todos.empty.day": "이 날의 할 일이 없어요 — 위 입력창에 적고 Enter.",
  "todos.meta.done": "{total}개 중 {done}개 완료",

  // 행 (grip·진행 배지·hover 액션)
  "todos.row.grip": "드래그해서 이동 · 가로로 깊이 조절",
  "todos.row.progress": "완료 하위 / 전체",
  "todos.row.addChild": "하위 추가",
  "todos.row.schedule": "시간표에 넣기",
  "todos.row.rename": "이름 변경",
  // 이월 고스트 뱃지 — 이 날짜에 있었지만 다른 날로 가져간 항목 (여기서 체크해도 완료된다)
  "todos.row.carriedTo": "{date}로 가져간 항목 — 여기서 체크해도 완료돼요",
  "todos.row.deleted": "삭제됨",
  "todos.row.deletedGhost": "지워진 항목 — 이 날짜에 있었다는 기록만 남습니다",

  // 하위 항목 추가 입력
  "todos.child.placeholder": "하위 항목 — Enter 로 추가",

  // 밀린 할 일 스트립
  "todos.overdue.title": "밀린 할 일",
  "todos.overdue.moveOne": "오늘로",
  "todos.overdue.moveAll": "모두 오늘로 가져오기",
  "todos.overdue.more": "외 {n}개",

  // 서브트리 삭제 확인 모달 — {name} 자리는 코드가 <b>제목</b> 으로 채운다(어순은 이 문자열이 결정)
  "todos.delete.title": "할 일 삭제",
  "todos.delete.confirm": "{name} 항목을 하위 {n}개와 함께 삭제할까요?",
  "todos.delete.irreversible": "되돌릴 수 없어요.",

  // 이날 학습완료 개념 칩
  "todos.learned.label": "이날 학습완료 {n}",
  "todos.learned.open": "개념 열기",

  // 미니 캘린더 pane — 화살표 라벨은 보고 있는 단계(일/월/연)에 따라 바뀐다
  "todos.cal.prevMonth": "이전 달",
  "todos.cal.nextMonth": "다음 달",
  "todos.cal.prevYear": "이전 해",
  "todos.cal.nextYear": "다음 해",
  "todos.cal.prevDecade": "이전 10년",
  "todos.cal.nextDecade": "다음 10년",
  "todos.cal.pickMonth": "월 선택",
  "todos.cal.pickYear": "연도 선택",
  "todos.cal.generating": "리포트 생성 중",

  // 공휴일 이름 (holidays.ts) — 달력 칸 아래 작은 글씨. 칸 폭이 좁아 넘치면 말줄임되므로
  // 되도록 짧게 쓴다(전체 이름은 칸 tooltip 으로 보인다).
  "todos.hol.newYear": "신정",
  "todos.hol.seollal": "설날",
  "todos.hol.seollalHoliday": "설 연휴",
  "todos.hol.independence": "삼일절",
  "todos.hol.buddha": "부처님오신날",
  "todos.hol.children": "어린이날",
  "todos.hol.memorial": "현충일",
  "todos.hol.constitution": "제헌절",
  "todos.hol.liberation": "광복절",
  "todos.hol.chuseok": "추석",
  "todos.hol.chuseokHoliday": "추석 연휴",
  "todos.hol.foundation": "개천절",
  "todos.hol.hangeul": "한글날",
  "todos.hol.christmas": "성탄절",
  "todos.hol.substitute": "대체공휴일",
  "todos.hol.temporary": "임시공휴일",
  "todos.hol.election": "선거일",

  // 타임테이블
  "todos.tt.label": "타임테이블",
  "todos.tt.view.day": "일",
  "todos.tt.view.week": "주",
  "todos.tt.view.month": "월",
  "todos.tt.planned": "계획 {time}",
  "todos.tt.hours": "{h}시간",
  "todos.tt.minutes": "{m}분",
  "todos.tt.titlePlaceholder": "제목",
  "todos.tt.untitled": "(제목 없음)",
  "todos.tt.deletedTodo": "(삭제된 할 일)",
  "todos.tt.deleteBlock": "블록 삭제",
  "todos.tt.monthEmpty": "이번 달 계획이 없어요 — 일/주 뷰에서 드래그로 추가하세요.",
} as const;

const en: Record<keyof typeof ko, string> = {
  "todos.today": "Today",
  "todos.nav.prevDay": "Previous day",
  "todos.nav.nextDay": "Next day",

  "todos.quick.placeholder": "Add a to-do and press Enter",
  "todos.empty.day": "No to-dos for this day — type one above and press Enter.",
  "todos.meta.done": "{done} of {total} done",

  "todos.row.grip": "Drag to move · horizontal to change depth",
  "todos.row.progress": "Sub-items done / total",
  "todos.row.addChild": "Add sub-item",
  "todos.row.schedule": "Add to timetable",
  "todos.row.rename": "Rename",
  "todos.row.carriedTo": "Moved to {date} — checking it here completes it too",
  "todos.row.deleted": "Deleted",
  "todos.row.deletedGhost": "Deleted — kept only as a record that it was here",

  "todos.child.placeholder": "Sub-item — press Enter to add",

  "todos.overdue.title": "Overdue",
  "todos.overdue.moveOne": "To today",
  "todos.overdue.moveAll": "Bring all to today",
  "todos.overdue.more": "{n} more",

  "todos.delete.title": "Delete to-do",
  "todos.delete.confirm": "Delete {name} and its {n} sub-item(s)?",
  "todos.delete.irreversible": "This can't be undone.",

  "todos.learned.label": "{n} learned on this day",
  "todos.learned.open": "Open concept",

  "todos.cal.prevMonth": "Previous month",
  "todos.cal.nextMonth": "Next month",
  "todos.cal.prevYear": "Previous year",
  "todos.cal.nextYear": "Next year",
  "todos.cal.prevDecade": "Previous 10 years",
  "todos.cal.nextDecade": "Next 10 years",
  "todos.cal.pickMonth": "Pick a month",
  "todos.cal.pickYear": "Pick a year",
  "todos.cal.generating": "Generating report",

  // 한국 공휴일 — en 도 칸 폭에 맞춰 짧게 (Seollal/Chuseok 은 고유명사라 그대로 음차)
  "todos.hol.newYear": "New Year",
  "todos.hol.seollal": "Seollal",
  "todos.hol.seollalHoliday": "Seollal",
  "todos.hol.independence": "March 1st",
  "todos.hol.buddha": "Buddha's Birthday",
  "todos.hol.children": "Children's Day",
  "todos.hol.memorial": "Memorial Day",
  "todos.hol.constitution": "Constitution Day",
  "todos.hol.liberation": "Liberation Day",
  "todos.hol.chuseok": "Chuseok",
  "todos.hol.chuseokHoliday": "Chuseok",
  "todos.hol.foundation": "Foundation Day",
  "todos.hol.hangeul": "Hangeul Day",
  "todos.hol.christmas": "Christmas",
  "todos.hol.substitute": "Substitute holiday",
  "todos.hol.temporary": "Temporary holiday",
  "todos.hol.election": "Election Day",

  "todos.tt.label": "Timetable",
  "todos.tt.view.day": "Day",
  "todos.tt.view.week": "Week",
  "todos.tt.view.month": "Month",
  "todos.tt.planned": "Planned {time}",
  "todos.tt.hours": "{h}h",
  "todos.tt.minutes": "{m}m",
  "todos.tt.titlePlaceholder": "Title",
  "todos.tt.untitled": "(No title)",
  "todos.tt.deletedTodo": "(deleted to-do)",
  "todos.tt.deleteBlock": "Delete block",
  "todos.tt.monthEmpty": "No plans this month — drag in the day/week view to add one.",
} as const;

export const todosMessages = { ko, en };
