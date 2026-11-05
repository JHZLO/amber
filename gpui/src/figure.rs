// 노트 그림(```svg, ```mermaid)을 웹 엔진 없이 그린다. svg 는 resvg 로, mermaid 는 crate::mermaid 로
// SVG 를 만든 뒤 같은 길로 간다. 레티나에서 흐리지 않게 2배로 굽고 절반 크기로 놓는다.

use std::sync::{Arc, OnceLock};

use gpui_kit::{Image, ImageFormat};
use resvg::{tiny_skia, usvg};

const SCALE: f32 = 2.0;

pub struct Figure {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// `text` 는 currentColor 가 가리킬 글자색 — 노트 그림은 색을 대부분 currentColor 로 받는다
pub fn svg(source: &str, text: &str) -> Result<Figure, String> {
    // width="100%" 는 브라우저 폭을 따르라는 뜻이라 여기선 viewBox 크기를 쓴다
    let source = source
        .replacen("<svg", &format!("<svg color=\"{text}\""), 1)
        .replace(" width=\"100%\"", "");
    let options = usvg::Options {
        fontdb: fonts(),
        font_family: "Apple SD Gothic Neo".into(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&source, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * SCALE).ceil() as u32,
        (size.height() * SCALE).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("빈 그림")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    Ok(Figure {
        image: Arc::new(Image::from_bytes(ImageFormat::Png, png)),
        width: size.width(),
        height: size.height(),
    })
}

/// 지금 앱처럼 밝은(neutral) 테마로 그린다 — 흰 판 위에 놓인다
pub fn mermaid(source: &str) -> Result<Figure, String> {
    svg(&amber_gpui::mermaid::to_svg(source)?, "#333333")
}
