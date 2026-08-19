-- v13: '주'의 시작 요일을 앱 전체에서 하나로 통일 — 일요일(lib/date.ts WEEK_STARTS_ON = 0).
--
-- 왜: 0011/0012 를 만들 때 주간 리포트·주 할 일은 스프린트 규약을 따라 **월요일** 시작으로,
-- 미니 캘린더와 타임테이블 주간 뷰는 기존대로 **일요일** 시작으로 두었다. 둘이 한 화면에
-- 나란히 놓이자 같은 '주'가 두 뜻을 갖게 됐다(캘린더는 일~토를 한 행으로 그리는데
-- 선택된 주는 월~일이라 띠가 두 행에 걸쳐 잘렸다).
--
-- 그래서 주 시작을 date.ts 의 상수 하나로 모으고, 이미 저장된 월요일 키를 그 주의
-- 일요일로 하루 당긴다. 이건 이 한 번의 정정이지, 상수를 바꿀 때마다 도는 마이그레이션이 아니다
-- (다시 월요일로 갈 거라면 반대 방향의 새 마이그레이션이 필요하다).
--
-- file_path 는 건드리지 않는다 — vault 의 실제 파일명은 SQL 이 못 바꾼다.
-- 대신 프론트가 행에 적힌 file_path 로 본문을 읽는다(lib/report.ts readWeeklyReportFile).

UPDATE weekly_reports
   SET week_start = date(week_start, '-1 day'),
       updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
 WHERE strftime('%w', week_start) = '1';  -- 월요일로 기록된 행만

UPDATE todos
   SET due_date = date(due_date, '-1 day'),
       updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
 WHERE scope = 'week' AND strftime('%w', due_date) = '1';
