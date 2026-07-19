-- v4: 할 일 상하위 관계 (1단계). parent_id 가 있으면 그 항목의 하위(자식).
-- 자식은 부모와 같은 due_date. 삭제 cascade 는 FK 대신 앱(lib/todos.ts)에서 명시 처리.
ALTER TABLE todos ADD COLUMN parent_id INTEGER;
CREATE INDEX idx_todos_parent ON todos (parent_id);
