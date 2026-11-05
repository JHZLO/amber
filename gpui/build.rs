// 마이그레이션 목록은 지금 앱의 src-tauri/src/lib.rs 가 정본이다. 두 벌로 적으면 등록을 빠뜨린 채 나가기 쉬워서
// (실제로 0016 이 등록 없이 태그까지 나갔다) 빌드할 때 그 목록을 읽어 그대로 싣는다 — 동등성 하네스와 같은 읽기다.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let lib = Path::new("../src-tauri/src/lib.rs");
    let dir = Path::new("../src-tauri/migrations");
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rerun-if-changed={}", dir.display());
    let src = std::fs::read_to_string(lib).expect("src-tauri/src/lib.rs");

    let mut found = Vec::new();
    for block in src.split("Migration {").skip(1) {
        let field = |name: &str| {
            let rest = &block[block.find(name)? + name.len()..];
            Some(
                rest.trim_start()
                    .trim_start_matches(':')
                    .trim_start()
                    .to_string(),
            )
        };
        let (Some(v), Some(d), Some(s)) = (
            field("version"),
            field("description"),
            field("include_str!("),
        ) else {
            continue;
        };
        let version: i64 = v[..v.find(',').unwrap()].trim().parse().expect("version");
        let description = d[1..d[1..].find('"').unwrap() + 1].to_string();
        let file = s.trim_start_matches(['(', '"']).split('"').next().unwrap();
        let file = file.trim_start_matches("../migrations/").to_string();
        found.push((version, description, file));
    }
    found.sort();
    assert!(
        !found.is_empty(),
        "src-tauri/src/lib.rs 에서 마이그레이션을 못 찾았다"
    );

    let mut out = String::from("pub const MIGRATIONS: &[(i64, &str, &str)] = &[\n");
    for (version, description, file) in found {
        let path = std::fs::canonicalize(dir.join(&file)).expect("migration file");
        writeln!(
            out,
            "    ({version}, {description:?}, include_str!({:?})),",
            path.display()
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("migrations.rs");
    std::fs::write(dest, out).unwrap();
}
