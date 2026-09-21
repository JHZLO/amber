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
  // 라이브 행이 사라진 기록 — '삭제됨' 같은 표식을 달지 않는다(오늘 지운 일을 어제 화면에
  // 스탬프로 찍지 않으려고, migrations/0014). 왜 흐린지는 툴팁으로만 말한다.
  "todos.row.goneRecord": "이 날 있었던 항목 — 지금은 목록에 없습니다",
  "todos.row.removeRecord": "이 날 기록에서 지우기",

  // 하위 항목 추가 입력
  "todos.child.placeholder": "하위 항목 — Enter 로 추가",

  // 밀린 할 일 스트립
  "todos.overdue.title": "밀린 할 일",

  // '언젠가' 서랍 — 달력에서 내려놓은 것. 끝내는 곳이 아니라 꺼내 오는 곳이라 체크박스가 없다
  "todos.parked.title": "언젠가",
  // 헤더 토글 — 탭이 둘이라 어느 한쪽 이름을 붙이면 나머지가 숨은 것처럼 읽힌다
  "todos.drawer.title": "서랍",

  // '오늘 후보' — AI 가 내 기록을 훑어 고른 것. 아직 할 일이 아니라 비우는 게 목표다
  "todos.suggest.title": "오늘 후보",
  "todos.suggest.run": "다시 훑기",
  "todos.suggest.running": "기록을 훑는 중…",
  "todos.suggest.idle": "아직 안 훑었어요. 위 새로고침을 누르면 밀린 일, 내려놓은 일, 최근 기록을 보고 오늘 챙길 것을 골라 드려요.",
  "todos.suggest.none": "오늘 따로 챙길 건 없어 보여요.",
  "todos.suggest.nothingToRead":
    "아직 읽을 게 없어요. 밀린 일이 생기거나, 할 일을 언젠가로 내려놓거나, 일간 리포트를 쓰면 그걸 보고 골라 드려요.",
  "todos.suggest.hint": "누르면 오늘 목록으로 들어가요. 안 고른 건 그냥 사라져요 — 따로 치우지 않아도 돼요.",
  "todos.suggest.acceptTip": "오늘 목록에 넣기",
  "todos.suggest.src.overdue": "밀린 일",
  "todos.suggest.src.anytime": "언젠가",
  "todos.suggest.src.note": "기록",
  "todos.suggest.src.other": "찾음",
  "todos.parked.open": "언젠가 열기",
  "todos.parked.close": "접기",
  "todos.parked.empty": "내려놓은 게 없어요. 오늘 안 할 일은 행 옆의 내려놓기로 여기에 둘 수 있어요.",
  "todos.parked.hint": "누르면 오늘로 올라와요. 날짜가 없으니 밀리지도, 개수에 세지지도 않아요.",
  "todos.parked.pullTip": "오늘로 올리기",
  "todos.parked.age.zero": "오늘",
  "todos.parked.age": "{n}일",
  "todos.parked.kids": "하위 {n}",
  "todos.row.park": "언젠가로 내려놓기",
  "todos.parked.pulled": "{name} 을(를) 오늘로 올렸어요",
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

  // 휴가 (vacations.ts) — 헤더 컨트롤(꺼짐=고스트 / 켜짐=노랑 칩) + 달력 라벨
  "todos.vac.set": "휴가",
  "todos.vac.change": "휴가 종류 변경",
  "todos.vac.clear": "휴가 해제",
  "todos.vac.kind.annual": "연차",
  "todos.vac.kind.half": "반차",
  "todos.vac.kind.sick": "병가",
  "todos.vac.kind.public": "공가",
  "todos.vac.kind.special": "특별휴가",

  // 타임테이블
  "todos.tt.label": "타임테이블",
  "todos.tt.view.day": "일",
  "todos.tt.view.week": "주",
  "todos.tt.view.month": "월",
  // 선택 단위 (미니 캘린더 위 토글) — 주 모드에서는 요일 없는 '이번 주' 할 일을 쓴다
  "todos.unit.day": "일",
  "todos.unit.week": "주",
  "todos.week.title": "{range} 주",
  "todos.week.add": "이번 주에 할 일을 적고 Enter",
  "todos.week.empty": "이 주에 적어 둔 할 일이 없어요 — 위에 입력하고 Enter 를 누르세요.",
  "todos.week.hint": "요일을 정하지 않은 '이번 주에 할 것'이에요. 날짜별 목록과는 따로 관리돼요.",
  "todos.week.done": "{total}개 중 {done}개 완료",
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
  "todos.row.goneRecord": "Was on this day — no longer in the list",
  "todos.row.removeRecord": "Remove from this day",

  "todos.child.placeholder": "Sub-item — press Enter to add",

  "todos.overdue.title": "Overdue",

  "todos.parked.title": "Anytime",
  "todos.drawer.title": "Drawer",

  "todos.suggest.title": "For today",
  "todos.suggest.run": "Look again",
  "todos.suggest.running": "Reading your records…",
  "todos.suggest.idle": "Nothing looked at yet. Refresh above and the overdue, the parked and the recent write-ups get read for what today is owed.",
  "todos.suggest.none": "Nothing stands out for today.",
  "todos.suggest.nothingToRead":
    "Nothing to read yet. Once something goes overdue, gets set down for Anytime, or a daily write-up exists, it gets read for what today is owed.",
  "todos.suggest.hint": "Click one to put it on today. What you leave simply goes - nothing to clear.",
  "todos.suggest.acceptTip": "Put on today",
  "todos.suggest.src.overdue": "Overdue",
  "todos.suggest.src.anytime": "Anytime",
  "todos.suggest.src.note": "Notes",
  "todos.suggest.src.other": "Found",
  "todos.parked.open": "Open Anytime",
  "todos.parked.close": "Collapse",
  "todos.parked.empty": "Nothing set down yet. Use the set-down action on a row to park what you are not doing today.",
  "todos.parked.hint": "Click one to bring it to today. With no date it never goes overdue and never counts.",
  "todos.parked.pullTip": "Bring to today",
  "todos.parked.age.zero": "today",
  "todos.parked.age": "{n}d",
  "todos.parked.kids": "{n} sub",
  "todos.row.park": "Set down for Anytime",
  "todos.parked.pulled": "Brought {name} to today",
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

  // 휴가 — en 도 칸 폭에 맞춰 짧게
  "todos.vac.set": "Time off",
  "todos.vac.change": "Change type",
  "todos.vac.clear": "Clear time off",
  "todos.vac.kind.annual": "Annual leave",
  "todos.vac.kind.half": "Half day",
  "todos.vac.kind.sick": "Sick leave",
  "todos.vac.kind.public": "Public duty",
  "todos.vac.kind.special": "Special leave",

  "todos.tt.label": "Timetable",
  "todos.tt.view.day": "Day",
  "todos.tt.view.week": "Week",
  "todos.tt.view.month": "Month",
  "todos.unit.day": "Day",
  "todos.unit.week": "Week",
  "todos.week.title": "Week of {range}",
  "todos.week.add": "Add something for this week and press Enter",
  "todos.week.empty": "Nothing noted for this week — type above and press Enter.",
  "todos.week.hint":
    "Things to do this week without picking a day. Kept separate from the daily lists.",
  "todos.week.done": "{done} of {total} done",
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
