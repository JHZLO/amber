---
description: 커밋 히스토리로 다음 버전을 판정하고 릴리스한다 (Amber)
argument-hint: [auto|patch|minor|major|x.y.z]
allowed-tools: Bash(node scripts/release.mjs*), Bash(git*), Bash(gh*)
---

Amber 릴리스를 수행한다. 인자: `$ARGUMENTS` (비어 있으면 분석만 하고 확인받는다).

## 1. 분석

지난 태그 이후 커밋으로 계산한 추천 버전:

!`node scripts/release.mjs`

## 2. 진행

- **인자가 비어 있으면**: 위 추천을 사용자에게 보여주고, 어떤 bump 로 갈지 확인받은 뒤 멈춘다.
- **인자가 있으면**(`auto` 포함): 아래 순서로 진행한다.

1. `node scripts/release.mjs $ARGUMENTS` — 버전이 박힌 4개 파일 갱신, 출력에서 새 버전 `vX.Y.Z` 확인
2. 커밋 + 태그:
   - `git commit -am "build: Bump version to vX.Y.Z"`
   - `git tag -a vX.Y.Z -m "Amber vX.Y.Z"`
3. **push·릴리스는 공개 액션이므로 사용자에게 확인**받는다. 확인되면:
   - `git push origin main --tags`
   - `gh release create vX.Y.Z --title "Amber vX.Y.Z" --generate-notes`
   - gh 계정을 여러 개 쓴다면, 이 레포를 소유한 계정으로 전환해 실행하고 끝나면 원래 계정으로 되돌린다.

## 정책 (요약)

- **0.x**: `feat` 또는 BREAKING → minor, 그 외 → patch. 1.0 승격은 수동 결정.
- 커밋은 conventional commits(`feat`/`fix`/`ref`/`docs`/`chore`/`build`/`ci`/…)를 따른다 — 자동 판정이 이 타입을 읽는다.
- 자세한 내용은 `AGENTS.md` 의 "버전 관리 & 릴리스" 참고.
