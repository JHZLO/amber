-- v14: 이월 기록을 자립시킨다 — 과거 날짜의 줄은 '지금 사는 행'을 가리키는 포인터가 아니라
-- 그 날의 스냅샷이다.
--
-- 0008 은 이월을 due_date UPDATE 한 번으로 처리하고 떠나온 날짜엔 (todo_id, date) 만 남겼다.
-- 그래서 **어제 줄은 곧 오늘 행**이었다 — 오늘 내용을 고치면 어제 기록의 텍스트까지 바뀌고,
-- 오늘 지우면 어제 줄이 CASCADE 로 사라졌다. 후자만 막으려고 0009 가 소프트 삭제(deleted_at)와
-- '삭제됨' 묘비를 얹었는데, 묘비는 내용도 없고 체크할 수도 없는 줄이라
--   · 그 날 부모 진행률의 분모에만 남아 부모가 영원히 안 닫히고
--   · 부모가 그 날 목록에 없으면 맥락 없이 최상위로 떠올랐다(고아)
--   · 사용자가 그 날에서 치울 방법도 없었다.
--
-- 그래서 기록이 내용을 직접 들고 있게 한다 — 이월 기록은 그 자체로 '그 날 이런 줄이 있었다'다:
--   · 삭제는 다시 하드 삭제 — deleted_at 도, 묘비 렌더 경로도 없다
--   · 편집 소급도 같이 사라진다 (과거 날짜는 항상 스냅샷 텍스트로 그린다)
--   · 라이브 행이 살아있는 동안은 여전히 같은 항목이다 — 고스트에서 체크하면 그 할 일이
--     완료되고(0008 의 요점) '→ 옮긴 날짜' 뱃지도 그대로다. 행이 사라지면 기록만 남아
--     읽기 전용 회색 줄이 된다.
--
-- todo_id 의 FK 를 뗀다 — 기록은 라이브 행보다 오래 살아야 한다(CASCADE 가 곧 0009 의 원인이었다).
-- todos.id 는 AUTOINCREMENT 라 id 가 재사용되지 않으므로, 떨어진 todo_id 가 다른 할 일을
-- 가리키게 되는 일은 없다.
CREATE TABLE todo_carries_new (
    todo_id   INTEGER NOT NULL,
    date      TEXT    NOT NULL
                      CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    -- 아래 세 개가 스냅샷 — 이월한 순간의 그 줄. content 는 그 날의 텍스트라 이후 편집에
    -- 따라가지 않고, done 은 마지막으로 알려진 상태다(하드 삭제 직전에 한 번 더 찍는다).
    content   TEXT    NOT NULL CHECK (length(content) > 0),
    parent_id INTEGER,
    done      INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1)),
    PRIMARY KEY (todo_id, date)
) STRICT;

INSERT INTO todo_carries_new (todo_id, date, content, parent_id, done)
SELECT c.todo_id, c.date, t.content, t.parent_id, t.done
  FROM todo_carries c JOIN todos t ON t.id = c.todo_id;

DROP TABLE todo_carries;
ALTER TABLE todo_carries_new RENAME TO todo_carries;
-- 날짜별 조회(그 날 보여줄 기록 찾기)에 대응. PK 는 todo_id 선두라 이 방향을 못 탄다.
CREATE INDEX idx_carries_date ON todo_carries (date);

-- 연동 블록도 같은 이유로 살려둔다: 할 일이 지워져도 '그 날 그 시간에 이걸 했다'는 기록이다.
-- CASCADE → SET NULL. 제목은 연동 블록이 '' 이라(할 일 내용을 미러) 떨어지기 전에 찍어야
-- 하는데, 그건 앱(lib/todos.ts deleteTodo)이 삭제 직전에 한다.
CREATE TABLE time_blocks_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    date       TEXT    NOT NULL
                       CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    start_min  INTEGER NOT NULL CHECK (start_min BETWEEN 0 AND 1435),
    end_min    INTEGER NOT NULL CHECK (end_min > start_min AND end_min <= 1440),
    title      TEXT    NOT NULL DEFAULT '',
    todo_id    INTEGER REFERENCES todos(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;

INSERT INTO time_blocks_new (id, date, start_min, end_min, title, todo_id, created_at, updated_at)
SELECT id, date, start_min, end_min, title, todo_id, created_at, updated_at FROM time_blocks;

DROP TABLE time_blocks;
ALTER TABLE time_blocks_new RENAME TO time_blocks;
CREATE INDEX idx_blocks_day ON time_blocks (date, start_min);

-- 이미 소프트 삭제된 행을 정리한다 — 위에서 기록이 내용을 챙겼으므로 남길 게 없다.
-- 블록은 제목을 먼저 찍어야 이름을 안 잃는다(FK 가 todo_id 만 떨군다).
UPDATE time_blocks
   SET title = (SELECT content FROM todos WHERE id = time_blocks.todo_id)
 WHERE title = ''
   AND todo_id IN (SELECT id FROM todos WHERE deleted_at IS NOT NULL);

-- 기록으로 남지 않는 날짜(= 그 항목이 살던 날짜)의 블록은 지운다 — 0009 경로가 이미 지웠지만,
-- 남아 있다면 주인 없는 블록이 되므로 여기서 정리한다.
DELETE FROM time_blocks
 WHERE todo_id IN (SELECT id FROM todos WHERE deleted_at IS NOT NULL)
   AND NOT EXISTS (
     SELECT 1 FROM todo_carries c
      WHERE c.todo_id = time_blocks.todo_id AND c.date = time_blocks.date
   );

DELETE FROM todos WHERE deleted_at IS NOT NULL;

-- deleted_at 자체를 걷어낸다. 부분 인덱스가 이 컬럼을 참조하므로 먼저 지운다.
DROP INDEX idx_todos_live;
DROP INDEX idx_todos_week;
ALTER TABLE todos DROP COLUMN deleted_at;
CREATE INDEX idx_todos_week ON todos (scope, due_date, sort_order, id);
