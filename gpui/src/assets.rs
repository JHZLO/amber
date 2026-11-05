// 아이콘 — 지금 앱의 icons.tsx 를 빌드할 때 SVG 로 구운 것(`amber/<이름>.svg`). 나머지(gpui-component 가
// 안에서 쓰는 아이콘)는 gpui-kit 의 전체 묶음에서 찾는다.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString, Styled, Svg, svg};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path
            .strip_prefix("amber/")
            .and_then(|p| p.strip_suffix(".svg"))
        {
            return Ok(ICONS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| Cow::Borrowed(s.as_bytes())));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::AllAssets.list(path)
    }
}

/// 지금 앱의 <Icon name=… size=…/> — 획 1.75, currentColor 는 text_color 가 정한다
pub fn icon(name: &str, size: f32) -> Svg {
    svg()
        .path(format!("amber/{name}.svg"))
        .size(gpui_kit::px(size))
}
