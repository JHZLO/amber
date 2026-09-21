-- v16: '언젠가' — 날짜 달력에서 내려놓은 할 일.
--
-- 왜 scope 에 값을 더하지 않았나: scope 의 CHECK 는 ('day','week') 로 컬럼에 붙어 있고(0012),
-- SQLite 는 CHECK 를 떼려면 테이블을 통째로 다시 만들어야 한다. todos 는 time_blocks 가
-- FK(ON DELETE SET NULL)로 물고 있고 트리거 둘과 인덱스 다섯이 달린 중심 테이블이라,
-- 값 하나 더하자고 DROP TABLE 을 태울 자리가 아니다.
--
-- 그리고 애초에 다른 질문이다. scope 는 "일 칸이냐 주 칸이냐"를 답하고,
-- parked_at 은 "지금 달력 위에 있기는 한가"를 답한다. 한 컬럼에 섞을 뜻이 아니다.
-- completed_at 이 '끝난 시각'으로 상태를 겸하는 것과 같은 꼴이다:
--   parked_at IS NULL     — 달력 위에 있다 (due_date 가 진짜 그 날)
--   parked_at IS NOT NULL — 내려놓았다. due_date 는 내려놓기 전 마지막 자리로 남고,
--                           목록의 '나이'는 now - parked_at 으로 센다
ALTER TABLE todos ADD COLUMN parked_at INTEGER;

-- '언젠가' 목록 전용 부분 인덱스. 내려놓은 것만 담으므로 대부분의 행을 안 싣는다.
CREATE INDEX idx_todos_parked ON todos (parked_at, sort_order, id)
    WHERE parked_at IS NOT NULL;
