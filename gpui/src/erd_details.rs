// 표 패널과 컬럼 정보창 그리기 — resin::details 의 나무를 viewer.css(.rz-details)의 규칙대로 칠한다.
// 색은 viewer.css 의 --rz-* 를 amber 가 styles.css 에서 덮은 값이다.
// 표(table)는 GPUI 에 없어 칸 폭을 잰다: 마지막 칸 앞은 글자 폭(+여백) 중 가장 넓은 것, 마지막 칸이 나머지를 갖는다.
// 브라우저의 자동 표 배치는 남는 폭을 칸마다 나누지만, 여기선 마지막 칸(설명, 관계)에 다 준다.

use amber_gpui::resin::details::{Act, El, Node};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::Spike;
use crate::day_card::Tag;
use crate::markdown::MONO;
use crate::theme::Tokens;

/// viewer.css 의 --rz-*(amber 가 덮은 값)
#[derive(Clone, Copy)]
pub struct Ink {
    pub ink: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub rule: Hsla,
    pub code: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub active_line: Hsla,
    pub pk: Hsla,
    pub uk: Hsla,
    pub fk: Hsla,
}

impl Ink {
    pub fn of(k: &Tokens) -> Ink {
        let c = |v: u32| -> Hsla { rgb(v).into() };
        let (accent, soft, pk, uk, fk) = if k.dark {
            (0xe2a64d, rgba(0xe2a64d24), 0xf6c76b, 0xc8bbff, 0x86e1ec)
        } else {
            (0x9a5a0c, rgba(0xcd8a1e24), 0xb26a00, 0x6d4fd8, 0x0b7a90)
        };
        Ink {
            ink: k.text,
            muted: k.text_2,
            faint: k.text_3,
            rule: k.border_strong,
            code: k.surface_2,
            accent: c(accent),
            accent_soft: soft.into(),
            active_line: k.text.opacity(0.04),
            pk: c(pk),
            uk: c(uk),
            fk: c(fk),
        }
    }

    /// 초점 CSS 에 넣을 강조색
    pub fn accent_hex(dark: bool) -> &'static str {
        if dark { "#e2a64d" } else { "#9a5a0c" }
    }
}

/// 글자 모양 — 부모에서 물려받고 클래스가 바꾼다
#[derive(Clone, Copy)]
struct Txt {
    mono: bool,
    size: f32,
    weight: FontWeight,
    color: Hsla,
}

impl Txt {
    fn font(&self) -> Font {
        Font {
            weight: self.weight,
            ..font(if self.mono {
                MONO
            } else {
                "Apple SD Gothic Neo"
            })
        }
    }
}

/// 요소 하나가 글자 모양을 어떻게 바꾸나(viewer.css 의 .rz-details 규칙)
fn restyle(el: &El, p: Txt, ink: &Ink) -> Txt {
    let mut t = p;
    let mono = |t: &mut Txt, size: f32| {
        t.mono = true;
        t.size = size;
    };
    if el.tag == "code" || el.has("d-link") {
        mono(&mut t, 12.);
    }
    if el.has("d-col-name") {
        mono(&mut t, 12.5);
    }
    if el.has("d-key") {
        mono(&mut t, 10.);
        t.weight = FontWeight::SEMIBOLD;
    }
    if el.has("t-name") || el.has("t-type") {
        t.mono = true;
    }
    if el.has("t-null") || el.has("t-note") {
        mono(&mut t, 11.);
    }
    if el.has("d-pop-name") {
        mono(&mut t, 13.);
        t.weight = FontWeight::SEMIBOLD;
    }
    if el.has("d-type") {
        mono(&mut t, 12.);
    }
    if el.has("t-none") || el.has("t-lab") {
        t.mono = false;
    }
    if el.has("t-none") {
        t.size = 12.;
    }
    if el.has("t-rel") {
        t.size = 12.;
    }
    // 색
    if el.has("t-type") || el.has("t-desc") || el.has("t-note") || el.has("d-type") {
        t.color = ink.muted;
    }
    if el.has("t-null") || el.has("t-none") || el.has("t-lab") || el.has("t-rel") || el.tag == "i" {
        t.color = ink.faint;
    }
    if el.has("is-null") {
        t.color = ink.pk;
        t.weight = FontWeight::SEMIBOLD;
    }
    if el.has("k-ix") {
        t.color = ink.muted;
    }
    if el.has("k-pk") {
        t.color = ink.pk;
    }
    if el.has("k-uk") {
        t.color = ink.uk;
    }
    if el.has("k-fk") {
        t.color = ink.fk;
    }
    if el.has("d-link") {
        t.color = ink.accent;
    }
    t
}

/// 한 줄로 놓았을 때의 폭 — 칸 폭을 정할 때
fn width(node: &Node, p: Txt, ink: &Ink, window: &Window) -> f32 {
    match node {
        Node::Text(s) => {
            if s.is_empty() {
                return 0.;
            }
            let run = TextRun {
                len: s.len(),
                font: p.font(),
                color: p.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(s.clone().into(), px(p.size), &[run], None)
                    .width,
            )
        }
        Node::El(el) => {
            let t = restyle(el, p, ink);
            let pad = if el.tag == "i" { 8. } else { 0. };
            pad + el
                .kids
                .iter()
                .map(|k| width(k, t, ink, window))
                .sum::<f32>()
        }
    }
}

/// 그리는 데 함께 들고 다니는 것
pub struct Ctx<'a> {
    pub ink: Ink,
    /// 정보창이 가리키는 컬럼 — 패널의 그 행을 칠한다(.is-on)
    pub on: Option<&'a str>,
    /// 누를 것에 붙일 id 의 머리
    pub id: &'a str,
}

fn act_id(ctx: &Ctx, act: &Act) -> ElementId {
    let s = match act {
        Act::Open(n) => format!("{}-open-{n}", ctx.id),
        Act::Pick(t, c) => format!("{}-pick-{t}.{c}", ctx.id),
        Act::Service(n) => format!("{}-svc-{n}", ctx.id),
    };
    ElementId::Name(s.into())
}

fn act_tag(act: &Act) -> String {
    match act {
        Act::Open(n) => format!("erd-link-{n}"),
        Act::Pick(t, c) => format!("erd-col-{t}.{c}"),
        Act::Service(n) => format!("erd-svc-{n}"),
    }
}

/// 글줄 안의 조각들(글자, 링크, 코드, 칩) — 조각 사이에서 줄이 바뀐다
fn inline(kids: &[Node], p: Txt, ctx: &Ctx, cx: &mut Context<Spike>) -> Div {
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .min_w_0()
        .font_family(if p.mono { MONO } else { "Apple SD Gothic Neo" })
        .text_size(px(p.size))
        .font_weight(p.weight)
        .text_color(p.color);
    for k in kids {
        row = row.child(piece(k, p, ctx, cx));
    }
    row
}

fn piece(node: &Node, p: Txt, ctx: &Ctx, cx: &mut Context<Spike>) -> AnyElement {
    match node {
        Node::Text(s) => div()
            .whitespace_nowrap()
            .child(s.clone())
            .into_any_element(),
        Node::El(el) => {
            let t = restyle(el, p, &ctx.ink);
            let mut d = inline(&el.kids, t, ctx, cx);
            if el.has("d-chip") {
                d = d
                    .px(px(6.))
                    .border_1()
                    .border_color(ctx.ink.rule)
                    .rounded(px(4.))
                    .bg(ctx.ink.code)
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .line_height(px(18.))
                    .text_color(ctx.ink.muted);
            }
            if el.tag == "i" {
                d = d.px(px(4.));
            }
            if el.has("d-key") || el.has("t-null") {
                d = d.line_height(px(18.));
            }
            if el.has("d-type") || el.has("t-null") {
                d = d.whitespace_nowrap();
            }
            match &el.act {
                Some(act) if el.tag == "button" => {
                    let a = act.clone();
                    d.id(act_id(ctx, act))
                        .tag(&act_tag(act))
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.erd_act(&a, window, cx);
                            cx.notify();
                        }))
                        .into_any_element()
                }
                _ if el.has("d-col-name") => d.hover(|s| s.underline()).into_any_element(),
                _ => d.into_any_element(),
            }
        }
    }
}

/// 표 이름 모양(머리말) — 대문자로
fn heading(text: &str, ink: &Ink) -> Div {
    div()
        .mb(px(6.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ink.faint)
        .child(text.to_uppercase())
}

fn kids_of<'a>(el: &'a El, tag: &str) -> impl Iterator<Item = &'a El> {
    el.kids.iter().filter_map(move |k| match k {
        Node::El(e) if e.tag == tag => Some(e),
        _ => None,
    })
}

fn first<'a>(el: &'a El, tag: &str) -> Option<&'a El> {
    kids_of(el, tag).next()
}

/// 패널의 표 — 칸 폭을 재서 줄마다 같은 폭으로 놓는다
fn table(
    t: &El,
    avail: f32,
    base: Txt,
    ctx: &Ctx,
    window: &Window,
    cx: &mut Context<Spike>,
) -> AnyElement {
    let ink = &ctx.ink;
    let heads: Vec<&El> = first(t, "thead")
        .and_then(|h| first(h, "tr"))
        .map(|tr| kids_of(tr, "th").collect())
        .unwrap_or_default();
    let rows: Vec<&El> = first(t, "tbody")
        .map(|b| kids_of(b, "tr").collect())
        .unwrap_or_default();
    let n = heads.len();
    // 칸 여백: 8px, 첫 칸 왼쪽과 마지막 칸 오른쪽은 6px
    let pad = |j: usize| (if j == 0 { 6. } else { 8. }) + (if j + 1 == n { 6. } else { 8. });
    let head_txt = Txt {
        mono: false,
        size: 10.5,
        weight: FontWeight::SEMIBOLD,
        color: ink.faint,
    };
    fn cell(tr: &El, j: usize) -> Option<&El> {
        kids_of(tr, "td").nth(j)
    }
    let mut widths: Vec<f32> = (0..n)
        .map(|j| {
            if heads[j].has("t-key") {
                return 26. + pad(j);
            }
            let head = heads[j]
                .kids
                .iter()
                .map(|k| width(k, head_txt, ink, window))
                .sum::<f32>();
            let body = rows
                .iter()
                .filter_map(|tr| cell(tr, j))
                .map(|td| {
                    let tt = restyle(td, base, ink);
                    td.kids
                        .iter()
                        .map(|k| width(k, tt, ink, window))
                        .sum::<f32>()
                })
                .fold(0., f32::max);
            head.max(body).ceil() + pad(j)
        })
        .collect();
    // 마지막 칸은 남는 폭을 갖는다 — 설명 칸은 120px 아래로 줄지 않는다
    let min_table = if t.has("t-cols") { 470. } else { 0. };
    let fixed: f32 = widths[..n.saturating_sub(1)].iter().sum();
    let last_min = if t.has("t-cols") {
        120. + pad(n - 1)
    } else {
        pad(n - 1) + 24.
    };
    let total = avail.max(min_table).max(fixed + last_min);
    if n > 0 {
        widths[n - 1] = total - fixed;
    }
    let mut out = div().w(px(total)).flex().flex_col().text_size(px(12.5));
    let head_row =
        div()
            .flex()
            .border_b_1()
            .border_color(ink.rule)
            .children(heads.iter().enumerate().map(|(j, th)| {
                div()
                    .w(px(widths[j]))
                    .flex_none()
                    .pl(px(if j == 0 { 6. } else { 8. }))
                    .pr(px(if j + 1 == n { 6. } else { 8. }))
                    .pb(px(6.))
                    .whitespace_nowrap()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(ink.faint)
                    .child(th.text())
            }));
    out = out.child(head_row);
    let last = rows.len().saturating_sub(1);
    let pickable = t.has("t-cols");
    for (i, tr) in rows.iter().enumerate() {
        let on = pickable && tr.c.is_some() && tr.c.as_deref() == ctx.on;
        let mut row = div()
            .flex()
            .when(i < last, |d| {
                d.border_b_1().border_color(ink.rule.opacity(0.55))
            })
            .when(on, |d| d.bg(ink.accent_soft));
        for j in 0..n {
            let Some(td) = cell(tr, j) else {
                continue;
            };
            let tt = restyle(td, base, ink);
            let mut c = inline(&td.kids, tt, ctx, cx)
                .w(px(widths[j]))
                .flex_none()
                .pl(px(if j == 0 { 6. } else { 8. }))
                .pr(px(if j + 1 == n { 6. } else { 8. }))
                .py(px(7.))
                .items_start();
            // 설명 칸 — 글은 한 덩어리(.t-desc-text)고 메모(.t-note)는 각자 한 줄
            if td.has("t-desc") {
                c = c.flex_col();
            }
            // 이름, 타입 칸은 줄을 바꾸지 않는다(white-space: nowrap) — 폭은 이미 쟀다
            if td.has("t-name") || td.has("t-type") {
                c = c.flex_nowrap();
            }
            row = row.child(c);
        }
        let row = match (&tr.act, pickable) {
            (Some(act), true) => {
                let a = act.clone();
                let hover = ink.active_line;
                row.id(act_id(ctx, act))
                    .tag(&act_tag(act))
                    .cursor_pointer()
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.erd_act(&a, window, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            _ => row.into_any_element(),
        };
        out = out.child(row);
    }
    // 좁으면 옆으로 굴린다(.t-wrap)
    div()
        .id(SharedString::from(format!(
            "{}-t-{}",
            ctx.id,
            heads.first().map(|h| h.text()).unwrap_or_default()
        )))
        .w_full()
        .overflow_x_scroll()
        .child(out)
        .into_any_element()
}

/// 블록 요소(머리, 절, 문단, 목록)
fn block(
    el: &El,
    avail: f32,
    base: Txt,
    ctx: &Ctx,
    window: &Window,
    cx: &mut Context<Spike>,
) -> AnyElement {
    let ink = &ctx.ink;
    let kids = |avail: f32, cx: &mut Context<Spike>| -> Vec<AnyElement> {
        el.kids
            .iter()
            .filter_map(|k| match k {
                Node::El(e) => Some(block(e, avail, base, ctx, window, cx)),
                Node::Text(_) => None,
            })
            .collect()
    };
    match el.tag {
        "div" if el.has("d-table") || el.has("d-pop") => div()
            .flex()
            .flex_col()
            .children(kids(avail, cx))
            .into_any_element(),
        "header" => div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .pb(px(14.))
            .border_b_1()
            .border_color(ink.rule)
            .children(kids(avail, cx))
            .into_any_element(),
        "h2" => div()
            .font_family(MONO)
            .text_size(px(18.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ink.ink)
            .child(el.text())
            .into_any_element(),
        "section" => {
            let mut s = div().pt(px(16.)).flex().flex_col();
            for k in &el.kids {
                if let Node::El(e) = k {
                    s = s.child(match e.tag {
                        "h3" => heading(&e.text(), ink).into_any_element(),
                        "div" => match first(e, "table") {
                            Some(t) => table(t, avail, base, ctx, window, cx),
                            None => div().into_any_element(),
                        },
                        _ => block(e, avail, base, ctx, window, cx),
                    });
                }
            }
            s.into_any_element()
        }
        "ul" => div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .children(kids(avail, cx))
            .into_any_element(),
        "li" => inline(&el.kids, Txt { size: 12.5, ..base }, ctx, cx)
            .px(px(6.))
            .py(px(4.))
            .line_height(relative(1.6))
            .into_any_element(),
        "span" if el.has("d-tags") => div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .children(el.kids.iter().map(|k| piece(k, base, ctx, cx)))
            .into_any_element(),
        "p" => {
            let t = if el.has("d-meta") {
                Txt {
                    size: 12.,
                    color: ink.faint,
                    ..base
                }
            } else {
                Txt {
                    size: 13.,
                    color: ink.muted,
                    ..base
                }
            };
            inline(&el.kids, t, ctx, cx)
                .when(el.has("d-desc") && ctx.id.ends_with("pop"), |d| {
                    d.mt(px(4.))
                })
                .into_any_element()
        }
        "div" if el.has("d-pop-head") => inline(&el.kids, base, ctx, cx)
            .gap(px(8.))
            .into_any_element(),
        "dl" => {
            // 두 칸 격자(이름, 값) — 이름 칸은 가장 긴 이름 폭
            let dt = Txt {
                size: 11.5,
                color: ink.faint,
                ..base
            };
            let names: Vec<&El> = kids_of(el, "dt").collect();
            let values: Vec<&El> = kids_of(el, "dd").collect();
            let w = names
                .iter()
                .map(|n| {
                    n.kids
                        .iter()
                        .map(|k| width(k, dt, ink, window))
                        .sum::<f32>()
                })
                .fold(0., f32::max)
                .ceil();
            let dd = Txt {
                mono: true,
                size: 12.,
                ..base
            };
            div()
                .mt(px(8.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .children(names.iter().zip(values).map(|(n, v)| {
                    div()
                        .flex()
                        .gap(px(12.))
                        .items_baseline()
                        .child(inline(&n.kids, dt, ctx, cx).w(px(w)).flex_none())
                        .child(inline(&v.kids, dd, ctx, cx).flex_1())
                }))
                .into_any_element()
        }
        _ => inline(&el.kids, base, ctx, cx).into_any_element(),
    }
}

/// 나무 하나를 그린다 — avail 은 안쪽 폭(표가 이보다 넓으면 옆으로 굴린다)
pub fn render(
    el: &El,
    avail: f32,
    ctx: &Ctx,
    window: &Window,
    cx: &mut Context<Spike>,
) -> AnyElement {
    let base = Txt {
        mono: false,
        size: 12.5,
        weight: FontWeight::NORMAL,
        color: ctx.ink.ink,
    };
    block(el, avail, base, ctx, window, cx)
}
