-- v3: 할 일 수동 정렬 순서 (드래그로 조정)
-- sort_order 오름차순이 표시 순서. 기존 행은 0 으로 채워져 id(생성순)로 정렬된다.
ALTER TABLE todos ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_todos_sort ON todos (due_date, sort_order, id);
