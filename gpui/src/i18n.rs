// 다국어 — 사전은 지금 앱의 src/lib/messages/*.ts 를 빌드할 때 그대로 굽는다(build.rs). 같은 키, 같은 문구.
// 언어는 지금 앱처럼 시스템 언어가 한국어면 ko, 아니면 en.

use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

fn dict() -> &'static HashMap<&'static str, &'static str> {
    static DICT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    DICT.get_or_init(|| {
        let ko = sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("ko"));
        (if ko { KO } else { EN }).iter().copied().collect()
    })
}

/// 지금 언어의 문구. 없는 키는 키 그대로(지금 앱과 같다)
pub fn t(key: &str) -> String {
    dict().get(key).copied().unwrap_or(key).to_string()
}
