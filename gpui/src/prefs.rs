// 화면 설정(언어, 테마, 마지막 탭, 판 너비, 작업 폴더 …). 지금 앱은 웹뷰의 localStorage 에 두는데, GPUI 앱에는
// 웹뷰가 없으니 같은 키 이름으로 DB 의 settings 표에 둔다. 처음 뜰 때 settings 에 아직 없는 키만 웹뷰 저장소에서
// 한 번 가져온다 — 지금 앱에서 고른 언어와 테마, 열어 둔 작업 폴더가 그대로 이어진다.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::concepts::{get_setting, set_setting};
use crate::db::Db;

/// 지금 앱이 localStorage 에 두는 키의 접두어
const PREFIXES: [&str; 2] = ["amber.", "til."];

pub fn get(db: &Db, key: &str) -> Option<String> {
    get_setting(db, key).ok().flatten()
}

pub fn set(db: &Db, key: &str, value: &str) {
    if let Err(e) = set_setting(db, key, value) {
        eprintln!("설정 {key} 을 못 썼다: {e}");
    }
}

/// 지금 앱(dev.jhzlo.amber)의 웹뷰 localStorage 파일
fn webview_store() -> Option<PathBuf> {
    let root = Path::new(&std::env::var_os("HOME")?)
        .join("Library/WebKit/dev.jhzlo.amber/WebsiteData/Default");
    // 옆에 salt 같은 파일도 있다 — 폴더만 본다
    for a in std::fs::read_dir(root).ok()?.flatten() {
        let Ok(inner) = std::fs::read_dir(a.path()) else {
            continue;
        };
        for b in inner.flatten() {
            let p = b.path().join("LocalStorage/localstorage.sqlite3");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// localStorage 의 값은 UTF-16LE 바이트다
fn read_store(path: &Path) -> rusqlite::Result<Vec<(String, String)>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = conn.prepare("SELECT key, value FROM ItemTable")?;
    let rows = stmt.query_map([], |r| {
        let key: String = r.get(0)?;
        let bytes: Vec<u8> = r.get(1)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok((key, String::from_utf16_lossy(&units)))
    })?;
    rows.collect()
}

/// settings 에 없는 키만 가져온다. 가져온 수
pub fn import_webview(db: &Db) -> usize {
    let Some(path) = webview_store() else {
        return 0;
    };
    import_from(db, &path)
}

fn import_from(db: &Db, path: &Path) -> usize {
    let items = match read_store(path) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("지금 앱의 화면 설정을 못 읽었다: {e}");
            return 0;
        }
    };
    let mut n = 0;
    for (key, value) in items {
        if PREFIXES.iter().any(|p| key.starts_with(p)) && get(db, &key).is_none() {
            set(db, &key, &value);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Clock;

    #[test]
    fn imports_only_missing_app_keys() {
        let dir = std::env::temp_dir().join(format!("amber-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("localstorage.sqlite3");
        let ls = Connection::open(&store).unwrap();
        ls.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB NOT NULL ON CONFLICT FAIL)")
            .unwrap();
        let utf16 = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<u8>>()
        };
        for (k, v) in [
            ("amber.lang", "en"),
            ("til.theme", "dark"),
            ("other", "x"),
            ("amber.root.notes", "/노트"),
        ] {
            ls.execute(
                "INSERT INTO ItemTable VALUES (?1, ?2)",
                rusqlite::params![k, utf16(v)],
            )
            .unwrap();
        }
        drop(ls);

        let db = Db::open(dir.join("amber.db").to_str().unwrap(), Clock::System).unwrap();
        crate::migrate::run(&db).unwrap();
        set(&db, "til.theme", "light");
        assert_eq!(import_from(&db, &store), 2);
        assert_eq!(get(&db, "amber.lang").as_deref(), Some("en"));
        assert_eq!(
            get(&db, "til.theme").as_deref(),
            Some("light"),
            "GPUI 앱에서 고른 값은 덮지 않는다"
        );
        assert_eq!(get(&db, "amber.root.notes").as_deref(), Some("/노트"));
        assert_eq!(get(&db, "other"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
