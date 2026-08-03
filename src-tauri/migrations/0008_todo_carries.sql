-- v8: 할 일 이월 이력 — '가져오기'로 날짜를 옮겨도 떠나온 날짜에 흔적을 남긴다.
-- 그 날짜를 열면 같은 행이 고스트로 다시 보인다(lib/todos.ts listTodos 의 UNION).
--
-- **행을 복제하지 않는다.** 이월은 todos.due_date UPDATE 하나뿐이고 여기엔 '이 할 일이 그
-- 날짜에 있었다'는 사실만 쌓는다. 할 일은 끝까지 한 행 = done 도 하나라서, 어제에서 체크하든
-- 오늘에서 체크하든 같은 항목이 완료된다 — 두 상태가 어긋나거나, 같은 내용이 두 줄로 늘어나
-- 밀린 목록에서 매일 불어나는 일이 없다.
--
-- 여러 번 이월하면 거쳐온 날짜마다 한 줄씩 쌓인다(월→화→수 = 월·화 두 줄). 반대로 만든 날
-- 방치했다가 며칠 뒤 가져오면 만든 날 한 줄만 — 실제로 목록에 있었던 날만 기록된다.
-- 도착 날짜(due_date)에는 기록을 남기지 않으므로 한 날짜에 같은 행이 두 번 나오지 않는다.
CREATE TABLE todo_carries (
    todo_id INTEGER NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    date    TEXT    NOT NULL
                    CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    PRIMARY KEY (todo_id, date)
) STRICT;

-- 날짜별 조회(그 날 보여줄 고스트 행 찾기)에 대응. PK 는 todo_id 선두라 이 방향을 못 탄다.
CREATE INDEX idx_carries_date ON todo_carries (date);
