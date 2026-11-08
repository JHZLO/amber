// 노트 그림(_assets) — 지금 앱 noteAssets.ts. 붙여넣거나 끌어다 놓은 그림을 노트와 같은 폴더의 `_assets/` 에
// 파일로 두고, 본문에는 `![](_assets/<이름>)` 상대 링크만 남긴다. 본문이 가리키는 파일 찾기(asset_refs),
// 그림 경로 풀기(resolve_local_src), 옮길 때 복사(copy_assets_for_move)는 vault.rs 에 있다.
//
// svg 는 받지 않는다 — 스크립트를 품을 수 있는 문서라 그림으로 들이지 않는다(노트의 그림은 ```svg 펜스가 맡는다).

use chrono::{Local, TimeZone};
use serde_json::Value as Json;

use crate::db::Db;
use crate::i18n::t;
use crate::vault::{self, ASSET_DIR};

type Result<T> = std::result::Result<T, String>;

/// 한 장의 상한 — 스크린샷은 보통 수 MB 라 넉넉하지만, 영상을 잘못 끌어다 놓는 걸 막는다
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

const KINDS: [(&str, &str); 4] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
];

pub fn ext_for_mime(mime: &str) -> Option<&'static str> {
    let m = mime.to_lowercase();
    KINDS.iter().find(|(k, _)| *k == m).map(|(_, e)| *e)
}

/// 파일 이름의 확장자로 그림 종류를 정한다. 그림이 아니면 None
pub fn mime_for_name(name: &str) -> Option<&'static str> {
    let (_, ext) = name.rsplit_once('.')?;
    match ext.to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// `20260924-153012-a1b2.png` — 시각이 앞이라 붙인 순서대로 정렬되고, 꼬리 네 글자가 같은 초에
/// 여러 장을 붙여도 겹치지 않게 한다. 시각은 현지 시각
pub fn asset_file_name(now_ms: i64, ext: &str, rand: &str) -> String {
    let at = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|d| d.format("%Y%m%d-%H%M%S").to_string())
        .unwrap_or_default();
    format!("{at}-{rand}.{ext}")
}

/// 0-9a-z 네 글자
fn random_suffix() -> String {
    let mut n = ulid::Ulid::new().random();
    (0..4)
        .map(|_| {
            let d = (n % 36) as u32;
            n /= 36;
            char::from_digit(d, 36).unwrap_or('0')
        })
        .collect()
}

/// 본문에 넣을 한 줄
pub fn image_markdown(file_name: &str) -> String {
    format!("![]({ASSET_DIR}/{file_name})")
}

/// 끼울 글 — 그림은 자기 줄에 혼자 서게 한다. 앞이 줄 중간이면 줄을 바꿔 시작하고, 뒤에 글이 이어지면
/// 줄을 바꿔 끝낸다. (앞 글자가 줄바꿈/처음인가, 뒤 글자가 줄바꿈/끝인가)
fn wrap_links(before_nl: bool, after_nl: bool, links: &[String]) -> String {
    let head = if before_nl { "" } else { "\n" };
    let tail = if after_nl { "" } else { "\n" };
    format!("{head}{}{tail}", links.join("\n"))
}

/// 고른 구간(바이트) 자리에 끼울 글
pub fn image_insert_text(value: &str, start: usize, end: usize, links: &[String]) -> String {
    wrap_links(
        start == 0 || value[..start].ends_with('\n'),
        end >= value.len() || value[end..].starts_with('\n'),
        links,
    )
}

/// 붙여넣거나 끌어다 놓은 그림을 `<dir>/_assets/` 에 쓰고 파일 이름을 돌려준다(dir = 앱 데이터 상대 노트 폴더)
pub fn save_note_image(db: &Db, dir: &str, data: &[u8], mime: &str, now_ms: i64) -> Result<String> {
    let ext = ext_for_mime(mime).ok_or_else(|| t("notes.image.unsupported"))?;
    if data.len() > MAX_IMAGE_BYTES {
        return Err(t("notes.image.tooLarge"));
    }
    let folder = db.at(&format!("{dir}/{ASSET_DIR}"));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name = asset_file_name(now_ms, ext, &random_suffix());
    std::fs::write(folder.join(&name), data).map_err(|e| e.to_string())?;
    Ok(name)
}

/// Finder 에서 끌어다 놓은 파일(절대경로)을 노트 폴더의 `_assets/` 로 복사한다
pub fn import_image_file(db: &Db, dir: &str, abs: &str, now_ms: i64) -> Result<String> {
    let name = abs.rsplit('/').next().unwrap_or(abs);
    let mime = mime_for_name(name).ok_or_else(|| t("notes.image.unsupported"))?;
    let data = std::fs::read(abs).map_err(|e| e.to_string())?;
    save_note_image(db, dir, &data, mime, now_ms)
}

/// 동등성 하네스 — noteAssets.* (자리는 JS 처럼 UTF-16)
pub fn call(db: &Db, op: &str, args: &[Json]) -> Option<Result<Json>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let n = |i: usize| args.get(i).and_then(Json::as_f64).unwrap_or(f64::NAN);
    let opt = |v: Option<&str>| v.map_or(Json::Null, Json::from);
    let out = match op {
        "noteAssets.extForMime" => Ok(opt(ext_for_mime(s(0)))),
        "noteAssets.mimeForName" => Ok(opt(mime_for_name(s(0)))),
        "noteAssets.assetFileName" => Ok(Json::from(asset_file_name(n(0) as i64, s(1), s(2)))),
        "noteAssets.imageMarkdown" => Ok(Json::from(image_markdown(s(0)))),
        "noteAssets.imageInsertText" => {
            let u: Vec<u16> = s(0).encode_utf16().collect();
            let len = u.len() as f64;
            let at = |i: f64| u.get(i as usize).copied();
            let (start, end) = (n(1), n(2));
            let links: Vec<String> = args
                .get(3)
                .and_then(Json::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            // value[start - 1] / value[end] — 범위 밖은 undefined(줄바꿈 아님), 처음과 끝은 줄바꿈으로 친다
            let before = !(start > 0.) || at(start - 1.) == Some(10);
            let after = !(end < len) || (end >= 0. && at(end) == Some(10));
            Ok(Json::from(wrap_links(before, after, &links)))
        }
        "noteAssets.assetRefs" => Ok(Json::from(vault::asset_refs(s(0)))),
        "noteAssets.resolveLocalSrc" => Ok(opt(vault::resolve_local_src(s(0), s(1)).as_deref())),
        "noteAssets.copyAssetsForMove" => {
            vault::copy_assets_for_move(db, s(0), s(1), s(2)).map(|_| Json::Null)
        }
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{image_insert_text, random_suffix};

    #[test]
    fn insert_text_keeps_the_figure_on_its_own_line_by_bytes() {
        let l = |s: &str| vec![s.to_string()];
        assert_eq!(
            image_insert_text("가나\n다", "가나".len(), "가나".len(), &l("L")),
            "\nL"
        );
        assert_eq!(image_insert_text("a\nb", 2, 2, &l("L")), "L\n");
        assert_eq!(image_insert_text("", 0, 0, &l("L")), "L");
    }

    #[test]
    fn the_suffix_is_four_base36_chars() {
        let s = random_suffix();
        assert_eq!(s.len(), 4);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
        );
    }
}
