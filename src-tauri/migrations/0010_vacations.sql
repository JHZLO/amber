-- v10: 휴가(연차·반차 등) — 사용자가 직접 표시하는 '쉬는 날'.
-- 공휴일(lib/holidays.ts)은 규칙으로 계산되는 사실이라 DB에 두지 않는다. 휴가는 반대로
-- 그 사람만 아는 정보라 저장이 정본이다 — 그래서 테이블은 여기 하나뿐이다.
--
-- 날짜가 곧 PK다: 하루에 휴가는 하나(연차이면서 동시에 병가일 수 없다). 종류를 바꾸는 건
-- UPSERT 한 번이고, 해제는 DELETE 한 번 — 상태가 '있다/없다' 둘뿐이라 토글이 어긋날 여지가 없다.
-- kind 는 표시 문자열이 아니라 코드다(annual/half/sick/…) — 화면 문구는 i18n 사전이 정한다.
CREATE TABLE vacations (
    date       TEXT PRIMARY KEY
                    CHECK (date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    kind       TEXT NOT NULL DEFAULT 'annual',
    created_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))
) STRICT;
