// app 도메인 문자열 — 키는 "app." 접두어 (충돌 방지). en 은 ko 와 키가 1:1 (누락 = 컴파일 에러).
// 대상: App.tsx (레일·상단바·개념 리스트) · SearchModal.tsx (⌘K) · Widget.tsx (위젯 창)

const ko = {
  // 레일(작업공간) 라벨 — 상단바 제목에도 재사용
  "app.rail.todo": "할 일",
  "app.rail.til": "개념",
  "app.rail.notes": "필기노트",
  "app.rail.diagrams": "다이어그램",
  "app.rail.settings": "설정",
  "app.rail.reportBusy": "리포트 생성 중…",
  "app.rail.reportBusyAria": "리포트 생성 중",

  // 상단바
  "app.search.placeholder": "검색 (제목·요약·태그)…",
  "app.add": "추가",
  "app.theme.toLight": "라이트 모드로",
  "app.theme.toDark": "다크 모드로",
  "app.widget.label": "위젯",
  "app.widget.open": "바탕화면 위젯 열기",

  // 개념 리스트 — 상태 탭(학습중/학습완료 는 common.status.* 재사용)·정렬·태그 필터
  "app.tab.all": "전체",
  "app.sort.canonical": "자신감 낮은 순",
  "app.sort.recentUpdated": "최근 수정순",
  "app.sort.recentCreated": "최근 추가순",
  "app.sort.title": "제목순",
  "app.filter.clear": "필터 초기화",

  // 빈 상태
  "app.empty.learning": "학습 중인 개념이 없어요.",
  "app.empty.noResults": "결과가 없어요.",
  "app.empty.addFirst": "첫 개념 추가",
  "app.empty.selectConcept": "개념을 선택하면 상세가 여기 나와요.",

  // 빠른 검색 (⌘K) — 종류 칩은 레일 라벨과 달리 단수 의미라 별도 키
  "app.quicksearch.title": "빠른 검색",
  "app.quicksearch.placeholder": "노트·다이어그램·개념 검색 (이름·본문)…",
  "app.quicksearch.hint": "↑↓ 이동 · Enter 열기 · Esc 닫기",
  "app.quicksearch.emptyPrompt": "이름이나 본문에 들어간 말을 입력해 보세요.",
  "app.quicksearch.searching": "찾는 중…",
  "app.quicksearch.kind.concept": "개념",
  "app.quicksearch.kind.note": "필기노트",
  "app.quicksearch.kind.diagram": "다이어그램",

  // 위젯 창
  "app.widget.emptyQueue": "학습 중인 개념이 없어요",
  "app.widget.openMain": "메인 열기",
  "app.widget.hide": "숨기기",
  "app.widget.prev": "이전",
  "app.widget.next": "다음",
  "app.widget.complete": "학습완료",
  "app.widget.confDown": "자신감 낮추기",
  "app.widget.confUp": "자신감 높이기",
  "app.widget.openDetail": "상세 열기",
} as const;

const en: Record<keyof typeof ko, string> = {
  "app.rail.todo": "To-dos",
  "app.rail.til": "Concepts",
  "app.rail.notes": "Notes",
  "app.rail.diagrams": "Diagrams",
  "app.rail.settings": "Settings",
  "app.rail.reportBusy": "Generating report…",
  "app.rail.reportBusyAria": "Generating report",

  "app.search.placeholder": "Search (title · summary · tags)…",
  "app.add": "Add",
  "app.theme.toLight": "Switch to light mode",
  "app.theme.toDark": "Switch to dark mode",
  "app.widget.label": "Widget",
  "app.widget.open": "Open desktop widget",

  "app.tab.all": "All",
  "app.sort.canonical": "Lowest confidence",
  "app.sort.recentUpdated": "Recently edited",
  "app.sort.recentCreated": "Recently added",
  "app.sort.title": "Title",
  "app.filter.clear": "Clear filters",

  "app.empty.learning": "No concepts in progress.",
  "app.empty.noResults": "No results.",
  "app.empty.addFirst": "Add your first concept",
  "app.empty.selectConcept": "Select a concept to see its details here.",

  "app.quicksearch.title": "Quick Search",
  "app.quicksearch.placeholder": "Search notes, diagrams, concepts (name · content)…",
  "app.quicksearch.hint": "↑↓ navigate · Enter open · Esc close",
  "app.quicksearch.emptyPrompt": "Type a word from a name or its content.",
  "app.quicksearch.searching": "Searching…",
  "app.quicksearch.kind.concept": "Concept",
  "app.quicksearch.kind.note": "Note",
  "app.quicksearch.kind.diagram": "Diagram",

  "app.widget.emptyQueue": "No concepts in progress",
  "app.widget.openMain": "Open main window",
  "app.widget.hide": "Hide",
  "app.widget.prev": "Previous",
  "app.widget.next": "Next",
  "app.widget.complete": "Mark as learned",
  "app.widget.confDown": "Lower confidence",
  "app.widget.confUp": "Raise confidence",
  "app.widget.openDetail": "Open details",
} as const;

export const appMessages = { ko, en };
