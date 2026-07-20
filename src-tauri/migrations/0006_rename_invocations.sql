-- v6: claude_invocations → ai_invocations 리네임.
-- AI 프로바이더 브리지(src/ai.rs)가 claude/codex/gemini 공용이라 호출 로그 테이블명도 벤더 중립으로.
-- 기존 로컬 DB: 이 마이그레이션만 실행되어 RENAME. 신규 DB: 0001 이 만든 테이블을 여기서 RENAME.
ALTER TABLE claude_invocations RENAME TO ai_invocations;
