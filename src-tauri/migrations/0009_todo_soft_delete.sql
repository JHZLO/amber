-- v9: 할 일 소프트 삭제 — 이월 흔적이 있는 항목은 지워도 '떠나온 날짜'의 기록은 남긴다.
--
-- 이월은 행을 복제하지 않는다(0008). 그래서 도착한 날짜에서 항목을 지우면 행이 사라지고,
-- ON DELETE CASCADE 로 todo_carries 까지 날아가 **어제 목록에서도 그 줄이 없어졌다.**
-- 어제 목록은 '어제 뭐가 있었는가'의 기록이라, 오늘 지운 일이 어제를 소급해 고쳐선 안 된다.
--
-- 그래서 이월 이력이 있는 항목은 행을 지우지 않고 deleted_at 을 찍는다:
--   · 지금 사는 날짜(due_date) 목록 · 밀린 목록 · 월 개수 — 전부에서 빠진다(= 지워진 것)
--   · 거쳐온 날짜에는 고스트로 남는다. 단 '삭제됨' 표식이라 체크할 수 없다(할 일이 없으니까)
-- 이력이 없는 항목은 남길 게 없으므로 예전처럼 행째로 지운다(lib/todos.ts deleteTodo).
ALTER TABLE todos ADD COLUMN deleted_at INTEGER;

-- 살아있는 행만 보는 조회(날짜별 목록·밀린 목록)가 지워진 행을 건너뛰게 한다.
CREATE INDEX idx_todos_live ON todos (due_date) WHERE deleted_at IS NULL;
