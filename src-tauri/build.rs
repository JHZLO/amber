use std::{env, fs, path::Path};

// SVG 스타일 가이드는 이 저장소에 없다. 각 기기의 `.agents/svg-style.md` 를 빌드 시점에 구워 넣고,
// 파일이 없으면 빈 문자열이 들어간다 — 그러면 `ai.rs` 의 note_prompt 가 아무것도 덧붙이지 않는다.
// 프롬프트 본문끼리는 서로를 모른다: 노트 프롬프트는 이 절이 있든 없든 혼자 완결된다.
fn main() {
    let guide = Path::new("../.agents/svg-style.md");
    // 없는 경로에 rerun-if-changed 를 걸면 cargo 가 매번 빌드 스크립트를 다시 돌린다(=매번 재컴파일).
    // 그래서 있을 때만 건다. 나중에 파일을 새로 만들면 그 한 번은 build.rs 를 건드려 굽는다.
    if guide.exists() {
        println!("cargo:rerun-if-changed=../.agents/svg-style.md");
    }
    let body = fs::read_to_string(guide).unwrap_or_default();
    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("svg-style.md");
    fs::write(&out, body).expect("svg-style.md 를 OUT_DIR 에 쓰지 못했다");

    tauri_build::build()
}
