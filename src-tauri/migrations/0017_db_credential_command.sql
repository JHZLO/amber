-- v17: 계정 명령 — 비밀번호를 저장하지 않고 접속할 때마다 받아 오는 연결.
--
-- 서비스마다 계정이 따로 있고 그 계정은 자기 스키마만 보는 DB 가 있다. 이런 곳은 계정 하나로
-- 스키마 여럿을 읽을 수 없고, 비밀번호도 비밀 저장소에서 바뀐다. 그래서 연결에 명령을 하나 두고
-- 스키마마다 실행해 {"username","password"} 를 받는다. 받은 값은 접속 풀 안에만 산다.
--   ''        — 지금까지처럼 username + 키체인 비밀번호
--   그 밖의 값 — 이 명령으로 스키마마다 계정을 받는다. `{schema}` 자리에 스키마 이름이 들어간다
ALTER TABLE db_connections ADD COLUMN credential_command TEXT NOT NULL DEFAULT '';
