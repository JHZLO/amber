-- v2: 할 일(TODO) — 날짜별 체크리스트
-- 타임스탬프 규칙: UTC epoch milliseconds (INTEGER, 0001 과 동일).
-- due_date 는 예외 — '사용자 로컬 달력의 날짜' TEXT('YYYY-MM-DD'). UTC ms 를 날짜로 재해석하면
-- 시간대 경계에서 하루 밀리므로, 캘린더 좌표는 렌더 시점의 로컬 날짜 문자열이 정본(lib/date.ts).

CREATE TABLE todos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    content      TEXT    NOT NULL CHECK (length(content) > 0),
    due_date     TEXT    NOT NULL
                         CHECK (due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    done         INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1)),
    completed_at INTEGER,                                        -- done 전이 시각(UTC ms), 트리거가 관리
    created_at   INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at   INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;

-- 일별 리스트 정렬(due_date=? ORDER BY done, id)에 정확히 대응
CREATE INDEX idx_todos_day  ON todos (due_date, done, id);
-- 밀린 할 일(미완료 과거) 조회 전용 부분 인덱스
CREATE INDEX idx_todos_open ON todos (due_date) WHERE done = 0;

-- 완료 시각 자동 정합 (0001 의 trg_learned_stamp / trg_relearn_clear 패턴).
-- 내부 UPDATE 는 completed_at/updated_at 만 건드리므로 'AFTER UPDATE OF done' 를 재발동시키지 않음.
CREATE TRIGGER trg_todo_done_stamp
AFTER UPDATE OF done ON todos FOR EACH ROW
WHEN NEW.done = 1 AND OLD.done = 0
BEGIN
    UPDATE todos
    SET completed_at = CAST(unixepoch('subsec') * 1000 AS INTEGER),
        updated_at   = CAST(unixepoch('subsec') * 1000 AS INTEGER)
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_todo_undone_clear
AFTER UPDATE OF done ON todos FOR EACH ROW
WHEN NEW.done = 0 AND OLD.done = 1
BEGIN
    UPDATE todos
    SET completed_at = NULL,
        updated_at   = CAST(unixepoch('subsec') * 1000 AS INTEGER)
    WHERE id = NEW.id;
END;
