// 지금 앱 소스에서 굽는 것 셋 — 두 벌로 적으면 갈라진다:
//  - 마이그레이션 목록(src-tauri/src/lib.rs)
//  - 아이콘(src/icons.tsx 의 PATHS) → OUT_DIR/icons.rs
//  - 다국어 사전(src/lib/messages/*.ts) → OUT_DIR/messages.rs
//
// 마이그레이션 목록은 지금 앱의 src-tauri/src/lib.rs 가 정본이다. 두 벌로 적으면 등록을 빠뜨린 채 나가기 쉬워서
// (실제로 0016 이 등록 없이 태그까지 나갔다) 빌드할 때 그 목록을 읽어 그대로 싣는다 — 동등성 하네스와 같은 읽기다.

use std::fmt::Write;
use std::path::Path;

fn main() {
    migrations();
    icons();
    messages();
}

fn out(name: &str, body: String) {
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join(name);
    std::fs::write(dest, body).unwrap();
}

/// icons.tsx 의 PATHS — 이름마다 JSX 조각(자기 닫는 svg 요소들)을 SVG 문서로. 속성은 SVG 이름으로(strokeWidth → stroke-width)
fn icons() {
    let src = "../src/icons.tsx";
    println!("cargo:rerun-if-changed={src}");
    let text = std::fs::read_to_string(src).expect("src/icons.tsx");
    let start = text.find("const PATHS").expect("PATHS");
    let body = &text[start..];
    let body = &body[body.find('{').unwrap() + 1..body.find("\n};").unwrap()];
    let key = regex::Regex::new(r#"(?m)^  "?([a-z0-9-]+)"?:\s"#).unwrap();
    let element = regex::Regex::new(r"<([a-z]+)\b([^>]*?)/>").unwrap();
    let attr = regex::Regex::new(r#"\s([a-zA-Z]+)=(?:\{([^}]*)\}|"([^"]*)")"#).unwrap();
    let kebab = |name: &str| {
        let mut s = String::new();
        for c in name.chars() {
            if c.is_ascii_uppercase() {
                s.push('-');
                s.push(c.to_ascii_lowercase());
            } else {
                s.push(c);
            }
        }
        s
    };
    let keys: Vec<_> = key
        .captures_iter(body)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();
    let mut code = String::from("pub const ICONS: &[(&str, &str)] = &[\n");
    for (i, (at, name)) in keys.iter().enumerate() {
        let end = keys.get(i + 1).map_or(body.len(), |k| k.0);
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">"#,
        );
        for el in element.captures_iter(&body[*at..end]) {
            svg.push('<');
            svg.push_str(&el[1]);
            for a in attr.captures_iter(&el[2]) {
                if &a[1] == "className" {
                    continue;
                }
                let v = a
                    .get(2)
                    .or(a.get(3))
                    .unwrap()
                    .as_str()
                    .trim()
                    .trim_matches('"');
                svg.push_str(&format!(r#" {}="{v}""#, kebab(&a[1])));
            }
            svg.push_str("/>");
        }
        svg.push_str("</svg>");
        writeln!(code, "    ({name:?}, {svg:?}),").unwrap();
    }
    code.push_str("];\n");
    out("icons.rs", code);
}

/// messages/*.ts 의 ko, en 객체 — "키": "값"(값은 JSON 과 같은 큰따옴표 문자열)
fn messages() {
    let dir = Path::new("../src/lib/messages");
    println!("cargo:rerun-if-changed={}", dir.display());
    let entry =
        regex::Regex::new(r#"(?m)^\s*"([A-Za-z0-9_.\-]+)"\s*:\s*("(?:[^"\\\n]|\\.)*")"#).unwrap();
    let (mut ko, mut en) = (String::new(), String::new());
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for f in files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "ts"))
    {
        let text = std::fs::read_to_string(f).unwrap();
        let split = text
            .find("const en")
            .unwrap_or_else(|| panic!("{}: const en 이 없다", f.display()));
        for (part, dst) in [(&text[..split], &mut ko), (&text[split..], &mut en)] {
            for c in entry.captures_iter(part) {
                let value: String = serde_json::from_str(&c[2]).unwrap_or_else(|e| {
                    panic!("{}: {} 의 값을 못 읽었다: {e}", f.display(), &c[1])
                });
                writeln!(dst, "    ({:?}, {value:?}),", &c[1]).unwrap();
            }
        }
    }
    out(
        "messages.rs",
        format!(
            "pub const KO: &[(&str, &str)] = &[\n{ko}];\npub const EN: &[(&str, &str)] = &[\n{en}];\n"
        ),
    );
}

fn migrations() {
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
    self::out("migrations.rs", out);
}
