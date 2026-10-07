use std::{env, fs, path::Path};

// 노트 AI 가 그림을 그릴 때 따르는 지침은 이 저장소에 없다. 각 기기의 로컬 파일을 빌드 시점에 구워 넣고,
// 파일이 없으면 빈 문자열이 들어간다 — 그러면 `ai.rs` 의 note_prompt 가 그 절을 덧붙이지 않는다.
// 프롬프트 본문끼리는 서로를 모른다: 노트 프롬프트는 이 절들이 있든 없든 혼자 완결된다.
//   svg-style.md       — SVG 스타일 가이드
//   architecture-svg.md — 아키텍처 그림 스킬. 노트 AI 는 Skill 도구를 못 쓰므로 본문을 굽는다(앞머리 메타데이터는 뺀다)
fn main() {
    bake("../.agents/svg-style.md", "svg-style.md");
    bake("../.agents/skills/architecture-svg/SKILL.md", "architecture-svg.md");
    tauri_build::build()
}

fn bake(src: &str, name: &str) {
    let path = Path::new(src);
    // 없는 경로에 rerun-if-changed 를 걸면 cargo 가 매번 빌드 스크립트를 다시 돌린다(=매번 재컴파일).
    // 그래서 있을 때만 건다. 나중에 파일을 새로 만들면 그 한 번은 build.rs 를 건드려 굽는다.
    if path.exists() {
        println!("cargo:rerun-if-changed={src}");
    }
    let body = fs::read_to_string(path).unwrap_or_default();
    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join(name);
    fs::write(&out, strip_front_matter(&body)).unwrap_or_else(|_| panic!("{name} 를 OUT_DIR 에 쓰지 못했다"));
}

// 스킬 파일 맨 앞의 `---` 메타데이터(name, description)는 스킬 목록용이다 — 프롬프트에는 본문만 싣는다
fn strip_front_matter(s: &str) -> &str {
    s.strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n").map(|end| &rest[end + 5..]))
        .unwrap_or(s)
}
