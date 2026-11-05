// 다국어 — 사전은 지금 앱의 src/lib/messages/*.ts 를 빌드할 때 그대로 굽는다(build.rs). 같은 키, 같은 문구.
// 언어는 지금 앱처럼 설정(amber.lang)이 있으면 그것, 없으면 시스템 언어가 한국어면 ko, 아니면 en.

use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

static KO_LANG: OnceLock<bool> = OnceLock::new();

/// 첫 문구를 그리기 전에 한 번 — 바꾸면 다시 시작해야 한다(지금 앱도 언어를 바꾸면 창을 다시 읽는다)
pub fn init(pref: Option<&str>) {
    let ko = match pref {
        Some("ko") => true,
        Some("en") => false,
        _ => sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("ko")),
    };
    let _ = KO_LANG.set(ko);
}

/// 지금 언어가 한국어인가 — 날짜 표기처럼 사전 밖에서 언어를 가르는 곳이 쓴다
pub fn is_ko() -> bool {
    *KO_LANG.get_or_init(|| {
        sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("ko"))
    })
}

fn dict() -> &'static HashMap<&'static str, &'static str> {
    static DICT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    DICT.get_or_init(|| (if is_ko() { KO } else { EN }).iter().copied().collect())
}

/// 지금 언어의 문구. 없는 키는 키 그대로(지금 앱과 같다)
pub fn t(key: &str) -> String {
    dict().get(key).copied().unwrap_or(key).to_string()
}
