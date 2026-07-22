-- v7: 시간 블록(타임테이블) — 할 일 탭 좌측 pane 하단의 구글 캘린더식 time-blocking.
-- 시간 좌표는 'UTC ms'가 아니라 "로컬 달력 날짜 TEXT + 자정 기준 분(0~1440)" —
-- 벽시계 좌표라 시간대/DST 경계에서 하루·한 시간 밀리는 문제가 없다(DESIGN.md §10).
-- todo_id 가 있으면 그 할 일의 '시간 배치'(제목은 할 일 내용을 미러) — 할 일이 지워지면 배치도 함께.
CREATE TABLE time_blocks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    date       TEXT    NOT NULL
                       CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    start_min  INTEGER NOT NULL CHECK (start_min BETWEEN 0 AND 1435),
    end_min    INTEGER NOT NULL CHECK (end_min > start_min AND end_min <= 1440),
    title      TEXT    NOT NULL DEFAULT '',                        -- 연동 블록은 '' (할 일 내용 표시)
    todo_id    INTEGER REFERENCES todos(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;

-- 일별 조회(date=? ORDER BY start_min)에 대응
CREATE INDEX idx_blocks_day ON time_blocks (date, start_min);
