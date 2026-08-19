-- v12: 주 단위 할 일 — '이번 주에 할 것'처럼 요일을 정하지 않는 항목.
--
-- due_date 는 그대로 재사용하되(주 항목은 **그 주 시작일**), scope 로 성격을 가른다.
-- 별도 테이블을 만들지 않는 이유: 하위 항목·정렬·완료 전이·소프트 삭제·드래그 이동이 전부
-- 지금 todos 위에 얹혀 있어서, 테이블을 나누면 그 로직을 통째로 두 벌 유지해야 한다.
--
-- **주의 — 이 컬럼이 생기는 순간 일별 조회가 새기 시작한다.** 주 항목의 due_date 는 실재하는
-- 날짜(그 주 시작일)라, 필터를 안 걸면 그 날의 목록·밀린 항목·캘린더 점에 그대로 섞인다.
-- 그래서 lib/todos.ts 의 listTodos / listMonthCounts / listOverdueOpen 은 전부
-- scope = 'day' 를 명시한다. 새 조회를 추가할 때도 반드시 scope 를 정하고 쓸 것.
--
-- 주의 시작 요일은 lib/date.ts 의 WEEK_STARTS_ON 하나로 정해진다 (0013 에서 일요일로 통일).
ALTER TABLE todos ADD COLUMN scope TEXT NOT NULL DEFAULT 'day'
    CHECK (scope IN ('day', 'week'));

-- 주 목록 조회(scope='week' AND due_date=<월요일> ORDER BY sort_order, id) 전용.
-- 기존 idx_todos_day 는 선두 컬럼이 due_date 라 scope 필터를 못 받는다.
CREATE INDEX idx_todos_week ON todos (scope, due_date, sort_order, id)
    WHERE deleted_at IS NULL;
