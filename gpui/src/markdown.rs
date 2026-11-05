// 노트의 글 덩어리를 TextView(gpui-component 마크다운)로 그리고, amber 고유 문법 둘을 얹는다.
//  - 알림 상자 `> [!NOTE]` (lib/mdAlerts.ts, Markdown.tsx 의 .md-alert): 종류 색 좌측 바 3px, 아이콘과 라벨, 본문은 평소 글자색
//  - 절 링크 `[[1-2|앞 절]]` (lib/mdSecRefs.ts): note::split 이 `[앞 절](#1-2)` 로 바꿔 두고, 누르면 on_link 가 받는다

use std::sync::Arc;

// gpui-component 의 TextView 는 감싸개라 제목 굵기, 여백, 색을 못 준다 — 안쪽 gpui-base TextView 에 바탕 스타일을 통째로 준다
use gpui_kit::base::text::{
    MarkdownNode, MarkdownParseContext, TextView, TextViewStyle, markdown_ast,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::assets::icon;
use crate::theme::Tokens;
use amber_gpui::i18n::t;

pub type OnLink = Arc<dyn Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + Send + Sync>;

/// 마커, 라벨 키, 아이콘(Markdown.tsx 의 ALERT_ICONS), 색(styles.css 의 --alert-* 라이트, 다크)
const ALERTS: [(&str, &str, &str, u32, u32); 5] = [
    ("NOTE", "common.alert.note", "info", 0x0969da, 0x79c0ff),
    ("TIP", "common.alert.tip", "lightbulb", 0x1a7f37, 0x56d364),
    (
        "IMPORTANT",
        "common.alert.important",
        "message",
        0x8250df,
        0xd2a8ff,
    ),
    (
        "WARNING",
        "common.alert.warning",
        "alert-triangle",
        0x9a6700,
        0xe3b341,
    ),
    (
        "CAUTION",
        "common.alert.caution",
        "alert-octagon",
        0xd1242f,
        0xff7b72,
    ),
];

struct Alert {
    kind: usize,
    body: SharedString,
}

/// 본문 고정폭 글꼴(--mono 의 ui-monospace 는 WebKit 에서 SF Mono)
pub const MONO: &str = "Menlo";

/// .markdown 의 글자 단계 — 지금 앱 웹 화면에서 잰 값(본문 14px, 줄 높이 1.7, 문단 위아래 8px)
fn note_style(k: &Tokens) -> TextViewStyle {
    let (text, text_2, strong) = (k.text, k.text_2, k.border_strong);
    let sr = StyleRefinement::default;
    TextViewStyle::default()
        .with_foreground(k.text)
        .with_muted_foreground(k.text_2)
        .with_link(k.text)
        .with_code_background(k.surface_2)
        .with_border(k.border)
        // 블록 사이 8px(p 의 margin 8px 이 겹쳐 접힌 값). 제목은 웹 여백에서 이 8px 을 뺀 만큼 더 띄운다
        .with_paragraph_gap(rems(0.5))
        .with_heading(move |level| match level {
            // h1: 21px 800, 왼쪽 굵은 바(4px) + 간격 10px — 대제목만 훑어볼 때 튀어나오게
            1 => sr()
                .text_size(px(21.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .line_height(relative(1.3))
                .mt(px(22.))
                .mb(px(10.))
                .border_l(px(4.))
                .border_color(text)
                .pl(px(10.)),
            2 => sr()
                .text_size(px(17.5))
                .font_weight(FontWeight::BOLD)
                .line_height(relative(1.35))
                .mt(px(18.)),
            3 => sr()
                .text_size(px(15.))
                .font_weight(FontWeight::BOLD)
                .line_height(relative(1.45))
                .mt(px(10.)),
            // h4 는 크기 차가 거의 없는 바닥 단계 — 명암으로 한 단 더 내린다
            _ => sr()
                .text_size(px(13.5))
                .font_weight(FontWeight::BOLD)
                .line_height(relative(1.5))
                .text_color(text_2)
                .mt(px(6.)),
        })
        .with_inline_code_style(
            // 좌우 여백 없이 — 지금 앱은 형광펜이 칩마다 끊기지 않게 세로 1px 만 둔다
            sr().font_family(MONO)
                .text_size(px(12.04))
                .bg(k.surface_2)
                .rounded(px(3.))
                .px(px(0.))
                .py(px(1.)),
        )
        .with_list_style(sr().text_color(text))
        // 인용 — 바 굵기가 위계다(대제목 4px > 알림 3px > 인용 2px)
        .with_blockquote_style(
            sr().text_color(text_2)
                .border_l(px(2.))
                .border_color(strong)
                .pl(px(14.)),
        )
        // 표는 내용 폭(display: block) — 칸이 내용에 맞고, 판보다 넓으면 가로로 굴린다. 모서리는 각지게
        .with_table(sr().rounded(px(0.)).self_start())
        .with_table_head(
            sr().bg(transparent_black())
                .text_color(text)
                .font_weight(FontWeight::BOLD),
        )
        .with_table_cell(sr().px(px(10.)).py(px(6.)))
        .with_dark(k.dark)
}

/// base = 노트 폴더의 실제 자리. `![](_assets/…)` 처럼 그 폴더 기준 상대경로 그림을 거기서 읽는다
pub fn text(
    id: impl Into<ElementId>,
    md: impl Into<SharedString>,
    on_link: OnLink,
    k: &Tokens,
    base: Option<SharedString>,
) -> TextView {
    let click = on_link.clone();
    let img_base = base.clone();
    TextView::markdown(id, md)
        .style(note_style(k))
        // 원격 URL 이나 기준 폴더가 없는 곳은 주소 그대로
        .image_source(move |uri| {
            match img_base
                .as_deref()
                .and_then(|b| amber_gpui::vault::resolve_local_src(b, uri.as_ref()))
            {
                Some(p) => ImageSource::from(std::sync::Arc::<std::path::Path>::from(
                    std::path::PathBuf::from(p),
                )),
                None => ImageSource::from(uri.clone()),
            }
        })
        .text_size(px(14.))
        .line_height(relative(1.7))
        .selectable(true)
        .on_link_click(move |url, ev, window, cx| click(url, ev, window, cx))
        .markdown_block_parser(parse_alert)
        .markdown_block_renderer("alert", move |node, _, cx| {
            render_alert(node, &Tokens::get(cx), on_link.clone(), base.clone())
        })
        .markdown_block_parser(parse_code)
        .markdown_block_renderer("code", |node, _, cx| render_code(node, &Tokens::get(cx)))
}

struct Code {
    lang: Option<String>,
    body: String,
}

/// 코드 펜스 → 창(.code-win). 그림 펜스(svg, mermaid)는 note::split 이 먼저 떼어 간다
fn parse_code(node: &markdown_ast::Node, _: &MarkdownParseContext) -> Option<MarkdownNode> {
    let markdown_ast::Node::Code(code) = node else {
        return None;
    };
    Some(MarkdownNode::new(
        "code",
        Code {
            lang: code.lang.clone().filter(|l| !l.is_empty()),
            body: code.value.clone(),
        },
    ))
}

/// (키워드, 문자열, 숫자, 함수, 타입, 주석) — styles.css 의 --code-kw ~ --code-dim
fn dark_palette(dark: bool) -> [u32; 6] {
    if dark {
        [0xd2a8ff, 0x7ee787, 0xffa657, 0x79c0ff, 0xffa198, 0x8b8b95]
    } else {
        [0x8250df, 0x0a7b34, 0xb3460f, 0x0550ae, 0x953800, 0x6b6b76]
    }
}

/// 줄마다 StyledText — 칠할 구간을 그 줄 안으로 잘라 옮긴다. 주석은 기울인다
fn code_lines(code: &Code, palette: [u32; 6]) -> Vec<StyledText> {
    use crate::highlight::{Kind, highlight};
    let spans = highlight(code.lang.as_deref(), &code.body);
    let body = code.body.strip_suffix('\n').unwrap_or(&code.body);
    let mut out = Vec::new();
    let mut start = 0;
    for line in body.split('\n') {
        let end = start + line.len();
        let hl: Vec<_> = spans
            .iter()
            .filter(|(r, _)| r.start < end && r.end > start)
            .map(|(r, kind)| {
                let color: Hsla = rgb(palette[match kind {
                    Kind::Keyword => 0,
                    Kind::Str => 1,
                    Kind::Num => 2,
                    Kind::Func => 3,
                    Kind::Type => 4,
                    Kind::Comment => 5,
                }])
                .into();
                let style = HighlightStyle {
                    color: Some(color),
                    font_style: (*kind == Kind::Comment).then_some(FontStyle::Italic),
                    ..Default::default()
                };
                (r.start.max(start) - start..r.end.min(end) - start, style)
            })
            .collect();
        // 빈 줄도 한 줄 높이를 지킨다
        out.push(if line.is_empty() {
            StyledText::new(" ")
        } else {
            StyledText::new(line.to_string()).with_highlights(hl)
        });
        start = end + 1;
    }
    out
}

/// 코드 블록 = macOS 창 — 신호등 막대 + 그 아래 코드. 장식이 아니라 경계다(본문 중간 코드의 시작과 끝)
fn render_code(node: &MarkdownNode, k: &Tokens) -> AnyElement {
    let Some(code) = node.data::<Code>() else {
        return div().into_any_element();
    };
    // --code-bg / --code-bar / --code-bar-line / --code-fg / --code-dim
    let (bg, bar, line, fg, dim) = if k.dark {
        (0x16161a, 0x1e1e23, 0x2a2a30, 0xe6e6ea, 0x8b8b95)
    } else {
        (0xf6f6f7, 0xececed, 0xe0e0e3, 0x1f1f23, 0x6b6b76)
    };
    let dot = |c: u32| div().size(px(11.)).rounded_full().bg(rgb(c));
    div()
        .my(px(4.))
        .border_1()
        .border_color(k.border)
        .rounded(px(10.))
        .overflow_hidden()
        .bg(rgb(bg))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .py(px(8.))
                .bg(rgb(bar))
                .border_b_1()
                .border_color(rgb(line))
                .child(dot(0xff5f57))
                .child(dot(0xfebc2e))
                .child(dot(0x28c840))
                .children(code.lang.clone().map(|l| {
                    div()
                        .ml_auto()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(dim))
                        .child(l)
                })),
        )
        .child(
            div()
                .px(px(14.))
                .py(px(12.))
                .font_family(MONO)
                .text_size(px(12.04))
                .line_height(px(20.47))
                .text_color(rgb(fg))
                .children(
                    code_lines(code, dark_palette(k.dark))
                        .into_iter()
                        .map(|l| div().whitespace_nowrap().child(l)),
                ),
        )
        .into_any_element()
}

/// mdAlerts.ts 의 matchAlertMarker — 마커는 자기 줄에 혼자 있어야 한다(`> [!NOTE] 본문` 은 일반 인용구)
fn alert_kind(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("[!")?;
    ALERTS.iter().position(|(marker, ..)| {
        rest.strip_prefix(marker)
            .and_then(|r| r.strip_prefix(']'))
            .map(|r| r.trim_start_matches([' ', '\t']))
            .is_some_and(|r| r.is_empty() || r.starts_with('\n') || r.starts_with("\r\n"))
    })
}

fn parse_alert(node: &markdown_ast::Node, cx: &MarkdownParseContext) -> Option<MarkdownNode> {
    let markdown_ast::Node::Blockquote(quote) = node else {
        return None;
    };
    let markdown_ast::Node::Paragraph(first) = quote.children.first()? else {
        return None;
    };
    let markdown_ast::Node::Text(lead) = first.children.first()? else {
        return None;
    };
    let kind = alert_kind(&lead.value)?;
    // 마커 줄을 떼고 줄마다 `>` 를 벗겨 본문 마크다운을 되살린다 — 본문의 목록, 코드, 링크를 그대로 그리려고
    let body: Vec<&str> = cx
        .node_source(node)?
        .lines()
        .skip(1)
        .map(|l| match l.trim_start().strip_prefix('>') {
            Some(r) => r.strip_prefix(' ').unwrap_or(r),
            None => l,
        })
        .collect();
    Some(MarkdownNode::new(
        "alert",
        Alert {
            kind,
            body: body.join("\n").into(),
        },
    ))
}

fn render_alert(
    node: &MarkdownNode,
    k: &Tokens,
    on_link: OnLink,
    base: Option<SharedString>,
) -> AnyElement {
    let dark = k.dark;
    let Some(alert) = node.data::<Alert>() else {
        return div().into_any_element();
    };
    let (_, label, icon_name, light_color, dark_color) = ALERTS[alert.kind];
    let color: Hsla = rgb(if dark { dark_color } else { light_color }).into();
    let id = ElementId::NamedInteger(
        "alert".into(),
        node.source_range().map_or(0, |r| r.start as u64),
    );
    div()
        .relative()
        .ml(px(17.))
        .py(px(2.))
        // 바는 왼쪽 여백 자리에 — .md-alert::before 와 같이, 본문을 끌어 고를 때 바까지 칠해지지 않게
        .child(
            div()
                .absolute()
                .left(px(-17.))
                .top_0()
                .bottom_0()
                .w(px(3.))
                .bg(color),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .mb(px(2.))
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(icon(icon_name, 14.).text_color(color))
                .child(t(label)),
        )
        .when(!alert.body.trim().is_empty(), |d| {
            d.child(text(id, alert.body.clone(), on_link, k, base))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::alert_kind;

    #[test]
    fn marker_must_stand_alone() {
        assert_eq!(alert_kind("[!NOTE]"), Some(0));
        assert_eq!(alert_kind("[!WARNING]  \n본문"), Some(3));
        assert_eq!(alert_kind("[!NOTE] 본문"), None);
        assert_eq!(alert_kind("[!note]"), None);
        assert_eq!(alert_kind("[!INFO]"), None);
    }
}
