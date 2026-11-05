// 노트의 글 덩어리를 TextView(gpui-component 마크다운)로 그리고, amber 고유 문법 둘을 얹는다.
//  - 알림 상자 `> [!NOTE]` (lib/mdAlerts.ts, Markdown.tsx 의 .md-alert): 종류 색 좌측 바 3px, 아이콘과 라벨, 본문은 평소 글자색
//  - 절 링크 `[[1-2|앞 절]]` (lib/mdSecRefs.ts): note::split 이 `[앞 절](#1-2)` 로 바꿔 두고, 누르면 on_link 가 받는다

use std::sync::Arc;

use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, TextView, markdown_ast};
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

pub fn text(id: impl Into<ElementId>, md: impl Into<SharedString>, on_link: OnLink) -> TextView {
    let click = on_link.clone();
    TextView::markdown(id, md)
        .selectable(true)
        .on_link_click(move |url, ev, window, cx| click(url, ev, window, cx))
        .markdown_block_parser(parse_alert)
        .markdown_block_renderer("alert", move |node, _, cx| {
            render_alert(node, Tokens::get(cx).dark, on_link.clone())
        })
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

fn render_alert(node: &MarkdownNode, dark: bool, on_link: OnLink) -> AnyElement {
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
            d.child(text(id, alert.body.clone(), on_link))
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
