//! 모델 → SVG(svg.ts 의 toSvg). 카드 크기를 재고 → ELK 로 배치하고(elk.rs, JavaScriptCore 의 elkjs) → SVG 글자.
//! 같은 입력이면 같은 SVG — 글자 폭은 재지 않고 규칙으로 정한다(고정폭 = 칸 x 0.6em, 비례 = 라틴 0.57em, CJK 1em).
//! 숫자는 JS 의 Number → 문자열과 Math.round 를 그대로 흉내 낸다(-0 은 "0", 반올림은 +무한대 쪽).
//! amber 가 지나지 않는 길은 옮기지 않았다: 서비스 접기(fold), diff 표시(change). amber 의 모델엔 둘 다 없다.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};

use super::{Model, ModelColumn, ModelService, ModelTable, RefKind, Relation, TableOrigin};

// ---- 옵션과 결과 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Columns {
    #[default]
    All,
    Keys,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditView {
    #[default]
    Collapse,
    Expand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edges {
    #[default]
    Angular,
    Curved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Right,
    Down,
}

/// 줄의 부품 — 이름은 늘 남는다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CardPart {
    Keys,
    Tags,
    Descriptions,
    Types,
    Nulls,
}

/// 줄이 그리는 순서
pub const CARD_PARTS: [CardPart; 5] = [
    CardPart::Keys,
    CardPart::Tags,
    CardPart::Descriptions,
    CardPart::Types,
    CardPart::Nulls,
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SvgOptions {
    pub columns: Columns,
    pub audit: AuditView,
    /// graphite(기본), aurora-dark, aurora-light, silk-dark, silk-light, caustic-dark, caustic-light
    pub look: String,
    pub standalone: bool,
    /// 유리 룩: 무대와 유리를 SVG 에 그린다(기본 true). 라이브 캔버스는 false
    pub stage: Option<bool>,
    pub edges: Edges,
    pub direction: Direction,
    pub hide: Vec<CardPart>,
    pub id_prefix: Option<String>,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            columns: Columns::All,
            audit: AuditView::Collapse,
            look: "graphite".into(),
            standalone: false,
            stage: None,
            edges: Edges::Angular,
            direction: Direction::Right,
            hide: Vec::new(),
            id_prefix: None,
        }
    }
}

/// 카드가 놓인 자리
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SvgBox {
    pub table: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SvgResult {
    pub svg: String,
    pub width: f64,
    pub height: f64,
    /// 무대 바탕색, graphite 면 None
    pub background: Option<String>,
    pub boxes: Vec<SvgBox>,
}

// ---- JS 숫자 ----

/// JS 의 String(number)
pub fn js(v: f64) -> String {
    if v == 0. {
        return "0".into();
    }
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v > 0. {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let a = v.abs();
    if !(1e-6..1e21).contains(&a) {
        let s = format!("{v:e}");
        let (m, e) = s.split_once('e').expect("지수 꼴");
        return if e.starts_with('-') {
            format!("{m}e{e}")
        } else {
            format!("{m}e+{e}")
        };
    }
    format!("{v}")
}

/// Math.round — 반은 +무한대 쪽으로
fn round(x: f64) -> f64 {
    let fl = x.floor();
    if x - fl >= 0.5 { fl + 1. } else { fl }
}

fn f(v: f64) -> f64 {
    round(v * 10.) / 10.
}
fn f3(v: f64) -> f64 {
    round(v * 1000.) / 1000.
}
fn up8(v: f64) -> f64 {
    (v / 8.).ceil() * 8.
}
fn max_of(xs: impl IntoIterator<Item = f64>, floor: f64) -> f64 {
    xs.into_iter().fold(floor, f64::max)
}

/// JSON 숫자 — 정수면 정수로(JS 의 JSON 과 같은 꼴)
fn num(v: f64) -> Json {
    if v.fract() == 0. && v.abs() < 9e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

// ---- 글자 폭 ----

const SANS: &str = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Geist', 'Segoe UI', 'Apple SD Gothic Neo', 'Noto Sans KR', system-ui, sans-serif";
const MONO: &str = "'SF Mono', 'Geist Mono', ui-monospace, Menlo, Consolas, monospace";

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11ff | 0x2e80..=0xa4cf | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6)
}
fn cells(s: &str) -> f64 {
    s.chars().map(|c| if is_wide(c) { 2. } else { 1. }).sum()
}
fn mono_w(s: &str, size: f64) -> f64 {
    cells(s) * size * 0.6
}
fn sans_w(s: &str, size: f64) -> f64 {
    s.chars()
        .fold(0., |w, c| w + if is_wide(c) { size } else { size * 0.57 })
}
/// JS 의 s.length
fn u16len(s: &str) -> f64 {
    s.encode_utf16().count() as f64
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---- 룩 ----

type Ink = (&'static str, f64);

struct Stage {
    style: &'static str,
    dark: bool,
    base: &'static str,
    colors: [Ink; 3],
    spots: [(f64, f64, f64); 3],
    grid: Ink,
    stars: Option<Ink>,
    grain: f64,
    vignette: f64,
}

struct Glass {
    frost: f64,
    veil: Ink,
    tint: Ink,
    rim: Ink,
    shadow: Ink,
}

struct Inks {
    text: Ink,
    muted: Ink,
    faint: Ink,
    ty: Ink,
    sep: Ink,
    head: Ink,
    pk: Ink,
    uk: Ink,
    fk: Ink,
    line: Ink,
    tag: Ink,
    tag_rim: Ink,
    tag_text: Ink,
}

struct Look {
    ink: &'static Inks,
    stage: Option<Stage>,
    glass: Option<&'static Glass>,
}

pub const GRID_STEP: f64 = 24.;
/// 카드 모서리 — 일부러 크지 않게(도식이지 장난감이 아니다)
pub const CARD_RADIUS: f64 = 8.;
const C: &str = "currentColor";
const WHITE: &str = "#FFFFFF";

static INK_CURRENT: Inks = Inks {
    text: (C, 1.),
    muted: (C, 0.6),
    faint: (C, 0.42),
    ty: (C, 0.6),
    sep: (C, 0.09),
    head: (C, 0.04),
    pk: (C, 1.),
    uk: (C, 0.78),
    fk: (C, 0.55),
    line: (C, 0.55),
    tag: (C, 0.04),
    tag_rim: (C, 0.18),
    tag_text: (C, 0.7),
};
static INK_DARK: Inks = Inks {
    text: ("#F6F7FB", 1.),
    muted: ("#DDE2F0", 0.62),
    faint: ("#DDE2F0", 0.42),
    ty: ("#DDE2F0", 0.66),
    sep: (WHITE, 0.09),
    head: (WHITE, 0.035),
    pk: ("#F6C76B", 1.),
    uk: ("#C8BBFF", 1.),
    fk: ("#86E1EC", 1.),
    line: ("#E9ECF8", 0.62),
    tag: (WHITE, 0.07),
    tag_rim: (WHITE, 0.16),
    tag_text: ("#E6EAF6", 0.8),
};
static INK_LIGHT: Inks = Inks {
    text: ("#0B1020", 1.),
    muted: ("#0B1020", 0.56),
    faint: ("#0B1020", 0.4),
    ty: ("#0B1020", 0.6),
    sep: ("#0F172A", 0.08),
    head: (WHITE, 0.3),
    pk: ("#B26A00", 1.),
    uk: ("#6D4FD8", 1.),
    fk: ("#0B7A90", 1.),
    line: ("#1E293B", 0.52),
    tag: (WHITE, 0.6),
    tag_rim: ("#0F172A", 0.12),
    tag_text: ("#0B1020", 0.64),
};
static DARK_GLASS: Glass = Glass {
    frost: 22.,
    veil: ("#0F121C", 0.74),
    tint: (WHITE, 0.07),
    rim: (WHITE, 0.16),
    shadow: ("#000000", 0.55),
};
static LIGHT_GLASS: Glass = Glass {
    frost: 22.,
    veil: ("#FAFBFD", 0.8),
    tint: (WHITE, 0.4),
    rim: ("#0F172A", 0.12),
    shadow: ("#1B2140", 0.18),
};

fn look_of(name: &str) -> Look {
    let glass = |dark: bool, stage: Stage| Look {
        ink: if dark { &INK_DARK } else { &INK_LIGHT },
        glass: Some(if dark { &DARK_GLASS } else { &LIGHT_GLASS }),
        stage: Some(stage),
    };
    match name {
        "aurora-dark" => glass(
            true,
            Stage {
                style: "aurora",
                dark: true,
                base: "#05060C",
                colors: [("#5A5FF0", 0.55), ("#22C7A9", 0.62), ("#E8A04A", 0.3)],
                spots: [(0.12, 0.12, 0.6), (0.92, 0.2, 0.55), (0.62, 1.0, 0.6)],
                grid: (WHITE, 0.1),
                stars: Some((WHITE, 0.95)),
                grain: 0.025,
                vignette: 0.35,
            },
        ),
        "aurora-light" => glass(
            false,
            Stage {
                style: "aurora",
                dark: false,
                base: "#E9EBF1",
                colors: [("#A9B1F7", 0.75), ("#8FD8CC", 0.7), ("#F7C893", 0.5)],
                spots: [(0.08, 0.1, 0.6), (0.95, 0.15, 0.55), (0.7, 1.0, 0.6)],
                grid: ("#1E2433", 0.1),
                stars: Some(("#6E74DA", 0.45)),
                grain: 0.02,
                vignette: 0.06,
            },
        ),
        "silk-dark" => glass(
            true,
            Stage {
                style: "silk",
                dark: true,
                base: "#0A0708",
                colors: [("#D9893A", 0.5), ("#5B2C7A", 0.55), ("#F1B6C4", 0.32)],
                spots: [(0.85, 0.85, 0.65), (0.15, 0.2, 0.6), (0.55, 0.45, 0.35)],
                grid: ("#FFF1E6", 0.09),
                stars: None,
                grain: 0.025,
                vignette: 0.4,
            },
        ),
        "silk-light" => glass(
            false,
            Stage {
                style: "silk",
                dark: false,
                base: "#F2ECE6",
                colors: [("#F2C28E", 0.62), ("#C9B8EE", 0.58), ("#FFFFFF", 0.55)],
                spots: [(0.85, 0.9, 0.65), (0.12, 0.15, 0.6), (0.5, 0.45, 0.35)],
                grid: ("#3A2A1C", 0.1),
                stars: None,
                grain: 0.02,
                vignette: 0.06,
            },
        ),
        "caustic-dark" => glass(
            true,
            Stage {
                style: "caustic",
                dark: true,
                base: "#031018",
                colors: [("#0E5E70", 0.6), ("#9FE7F2", 0.3), ("#F3D49A", 0.2)],
                spots: [(0.2, 0.25, 0.7), (0.75, 0.4, 0.5), (0.5, 1.05, 0.5)],
                grid: ("#DFF7FF", 0.09),
                stars: None,
                grain: 0.025,
                vignette: 0.4,
            },
        ),
        "caustic-light" => glass(
            false,
            Stage {
                style: "caustic",
                dark: false,
                base: "#E4F1F4",
                colors: [("#9FD6E0", 0.6), ("#FFFFFF", 0.5), ("#F4E2C4", 0.45)],
                spots: [(0.2, 0.2, 0.65), (0.7, 0.45, 0.5), (0.55, 1.05, 0.5)],
                grid: ("#123A44", 0.1),
                stars: None,
                grain: 0.02,
                vignette: 0.06,
            },
        ),
        _ => Look {
            ink: &INK_CURRENT,
            stage: None,
            glass: None,
        },
    }
}

fn fill((c, a): Ink) -> String {
    if a == 1. {
        format!("fill=\"{c}\"")
    } else {
        format!("fill=\"{c}\" fill-opacity=\"{}\"", js(a))
    }
}
fn stroke((c, a): Ink) -> String {
    if a == 1. {
        format!("stroke=\"{c}\"")
    } else {
        format!("stroke=\"{c}\" stroke-opacity=\"{}\"", js(a))
    }
}
fn stop(offset: f64, (c, a): (&str, f64)) -> String {
    format!(
        "<stop offset=\"{}\" stop-color=\"{c}\" stop-opacity=\"{}\"/>",
        js(offset),
        js(a)
    )
}

// ---- 카드 치수(8 단위 격자) ----

const PAD: f64 = 16.;
const HEAD: f64 = 44.;
const ROW: f64 = 28.;
const FOOT: f64 = 18.;
const RX: f64 = CARD_RADIUS;
/// 컬럼 이름 앞 키 표(PK, UK, FK)의 자리
const KEY: f64 = 26.;
/// NOT NULL 과 널이 되는 컬럼이 같은 행을 가리킬 때 첫 구간을 위아래로 벌리는 거리
const SPLIT: f64 = 6.;
/// 곡선 연결선이 꺾기 전에 카드에서 곧게 나가는 거리
const LEAD: f64 = 18.;
const TAG_SIZE: f64 = 8.5;

struct View<'a> {
    table: &'a ModelTable,
    shown: Vec<&'a ModelColumn>,
    /// 관계의 자식 쪽 컬럼
    ref_cols: HashSet<String>,
    tag: Option<String>,
    foot: Vec<String>,
    hide: HashSet<CardPart>,
    w: f64,
    h: f64,
    name_w: f64,
}

impl View<'_> {
    fn show(&self, p: CardPart) -> bool {
        !self.hide.contains(&p)
    }
    /// 머리 아래 아무것도 없는 카드 — 이름만 보기
    fn bare(&self) -> bool {
        self.shown.is_empty() && self.foot.is_empty()
    }
}

fn chips(c: &ModelColumn, v: &View) -> Vec<&'static str> {
    let mut out = Vec::new();
    if c.pk && v.ref_cols.contains(&c.name) {
        out.push("FK");
    }
    if c.enum_values.is_some() {
        out.push("ENUM");
    }
    if c.enc {
        out.push("ENC");
    }
    if c.index.is_some() {
        out.push("IX");
    }
    out
}
fn chip_w(text: &str) -> f64 {
    mono_w(text, 8.5) + 10.
}
fn tag_w(text: &str) -> f64 {
    mono_w(text, TAG_SIZE) + u16len(text) * 0.34
}
fn tags_w(list: &[&str]) -> f64 {
    if list.is_empty() {
        0.
    } else {
        list.iter().fold(0., |w, x| w + tag_w(x)) + 7. + 6. * (list.len() as f64 - 1.)
    }
}
/// 널은 DDL 처럼 적는다. 만든 감사 표는 빼고
fn is_null(c: &ModelColumn, t: &ModelTable) -> bool {
    c.nullable && t.origin != TableOrigin::Audit
}
fn null_w() -> f64 {
    chip_w("NULL") + 8.
}

fn row_title(c: &ModelColumn, t: &ModelTable) -> String {
    let null = if t.origin == TableOrigin::Audit {
        ""
    } else if c.nullable {
        " NULL"
    } else {
        " NOT NULL"
    };
    let mut parts = vec![format!("{} {}{null}", c.name, c.ty)];
    if let Some(d) = c.description.as_ref().filter(|d| !d.is_empty()) {
        parts.push(d.clone());
    }
    if let Some(v) = &c.enum_values {
        parts.push(v.join(" / "));
    }
    if let Some(n) = c.uk_name.as_ref().filter(|n| !n.is_empty()) {
        parts.push(format!("unique {n}"));
    }
    if let Some(ix) = &c.index {
        parts.push(format!(
            "index{}",
            ix.name
                .as_ref()
                .filter(|n| !n.is_empty())
                .map(|n| format!(" {n}"))
                .unwrap_or_default()
        ));
    }
    if let Some(r) = &c.reference {
        let arrow = if r.kind == RefKind::Physical {
            "->"
        } else {
            "~>"
        };
        parts.push(format!("{arrow} {}.{}", r.table, r.column));
    }
    parts.join("\n")
}

fn label_of(t: &ModelTable) -> &str {
    t.label.as_deref().unwrap_or(&t.name)
}

fn measure(v: &mut View) {
    let t = v.table;
    let name_w = max_of(
        v.shown.iter().map(|c| {
            sans_w(&c.name, 12.5) * if c.pk { 1.04 } else { 1. }
                + if v.show(CardPart::Tags) {
                    tags_w(&chips(c, v))
                } else {
                    0.
                }
        }),
        48.,
    );
    let desc_w = if v.show(CardPart::Descriptions) {
        max_of(
            v.shown.iter().map(|c| {
                c.description
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map_or(0., |d| sans_w(d, 11.5))
            }),
            0.,
        )
    } else {
        0.
    };
    let type_w = if v.show(CardPart::Types) {
        max_of(v.shown.iter().map(|c| mono_w(&c.ty, 11.)), 40.)
    } else {
        0.
    };
    let nw = if v.show(CardPart::Nulls) && v.shown.iter().any(|c| is_null(c, t)) {
        null_w()
    } else {
        0.
    };
    let row_w = PAD * 2.
        + if v.show(CardPart::Keys) { KEY } else { 0. }
        + name_w
        + if desc_w != 0. { 14. + desc_w } else { 0. }
        + 18.
        + type_w
        + nw;
    let head_w = PAD * 2.
        + sans_w(label_of(t), 13.5) * 1.04
        + 10.
        + t.description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map_or(0., |d| sans_w(d, 12.))
        + v.tag.as_deref().map_or(0., |g| 14. + chip_w(g));
    let foot_w = PAD * 2. + max_of(v.foot.iter().map(|s| mono_w(s, 9.5)), 0.);
    v.name_w = name_w;
    // 머리뿐인 카드는 이름만큼 — 이름만 보기는 많은 표를 한눈에 보려는 것
    if v.bare() {
        v.w = up8(head_w.max(120.));
        v.h = HEAD;
        return;
    }
    v.w = up8(row_w.max(head_w).max(foot_w).max(200.));
    v.h = up8(HEAD
        + v.shown.len() as f64 * ROW
        + if v.foot.is_empty() {
            0.
        } else {
            v.foot.len() as f64 * FOOT + 8.
        }
        + 8.);
}

fn row_y(i: usize) -> f64 {
    HEAD + 4. + i as f64 * ROW + ROW / 2.
}
/// 연결선이 카드에 닿는 높이 — 컬럼의 행, 머리뿐이면 머리 가운데
fn anchor_y(v: &View, column: &str) -> f64 {
    if v.bare() {
        HEAD / 2.
    } else {
        // findIndex 가 -1 이면 그 자리(-1)로 셈한다
        let i = v.shown.iter().position(|c| c.name == column);
        match i {
            Some(i) => row_y(i),
            None => HEAD + 4. - ROW + ROW / 2.,
        }
    }
}
fn port_of<'a>(v: &View, column: &'a str) -> &'a str {
    if v.bare() { "" } else { column }
}
/// 머리 띠: 위 모서리만 둥글게
fn cap_path(w: f64) -> String {
    format!(
        "M0,{rx} A{rx},{rx} 0 0 1 {rx},0 H{} A{rx},{rx} 0 0 1 {},{rx} V{} H0 Z",
        js(w - RX),
        js(w),
        js(HEAD),
        rx = js(RX)
    )
}

fn chip(text: &str, x: f64, y: f64, i: &Inks) -> String {
    let w = chip_w(text);
    format!(
        "<g transform=\"translate({},{})\"><rect width=\"{}\" height=\"14\" rx=\"3\" {} {}/><text x=\"{}\" y=\"10\" text-anchor=\"middle\" font-family=\"{MONO}\" font-size=\"8.5\" font-weight=\"500\" {}>{}</text></g>",
        js(f(x)),
        js(f(y)),
        js(f(w)),
        fill(i.tag),
        stroke(i.tag_rim),
        js(f(w / 2.)),
        fill(i.tag_text),
        esc(text)
    )
}

fn key_label(c: &ModelColumn, v: &View, i: &Inks) -> Option<(&'static str, Ink)> {
    if c.pk {
        Some(("PK", i.pk))
    } else if c.uk {
        Some(("UK", i.uk))
    } else if v.ref_cols.contains(&c.name) {
        Some(("FK", i.fk))
    } else {
        None
    }
}

/// 판 — 파일 속 유리는 베일, 씻김, 고른 테. graphite 는 잉크만. 라이브 캔버스는 페이지가 칠하니 여기선 비운다
fn panel(v: &View, l: &Look, id: &dyn Fn(&str) -> String, painted: bool) -> String {
    let (w, h) = (v.w, v.h);
    let dash = if v.table.origin == TableOrigin::External {
        " stroke-dasharray=\"5 4\""
    } else {
        ""
    };
    if !painted {
        return if dash.is_empty() {
            String::new()
        } else {
            format!(
                "<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\" {}{dash}/>",
                js(w - 1.),
                js(h - 1.),
                js(RX - 0.5),
                stroke(l.ink.sep)
            )
        };
    }
    let mut s = String::new();
    if let Some(g) = l.glass {
        s += &format!(
            "<rect width=\"{}\" height=\"{}\" rx=\"{}\" {}/>",
            js(w),
            js(h),
            js(RX),
            fill(g.veil)
        );
    }
    s += &format!(
        "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"url(#{})\"/>",
        js(w),
        js(h),
        js(RX),
        id("wash")
    );
    s += &if v.bare() {
        format!(
            "<rect width=\"{}\" height=\"{}\" rx=\"{}\" {}/>",
            js(w),
            js(h),
            js(RX),
            fill(l.ink.head)
        )
    } else {
        format!("<path d=\"{}\" {}/>", cap_path(w), fill(l.ink.head))
    };
    let rim = l.glass.map_or((C, 0.25), |g| g.rim);
    s += &format!(
        "<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\" {}{dash}/>",
        js(w - 1.),
        js(h - 1.),
        js(RX - 0.5),
        stroke(rim)
    );
    s
}

fn card(v: &View, l: &Look, id: &dyn Fn(&str) -> String, painted: bool) -> String {
    let t = v.table;
    let i = l.ink;
    let w = v.w;
    let mut s = vec![panel(v, l, id, painted)];
    // 머리 — 한 묶음에 제 누를 자리를 둔다(이름, 설명, 꼬리표 어디를 눌러도)
    let hit = if v.bare() {
        format!(
            "<rect class=\"rz-hit\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"currentColor\" fill-opacity=\"0\"/>",
            js(w),
            js(HEAD),
            js(RX)
        )
    } else {
        format!(
            "<path class=\"rz-hit\" d=\"{}\" fill=\"currentColor\" fill-opacity=\"0\"/>",
            cap_path(w)
        )
    };
    s.push(format!("<g class=\"rz-head\">{hit}"));
    s.push(format!(
        "<text x=\"{}\" y=\"27\" font-size=\"13.5\" font-weight=\"600\" letter-spacing=\"-0.01em\" {}>{}{}</text>",
        js(PAD),
        fill(i.text),
        esc(label_of(t)),
        t.description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| format!(
                "<tspan dx=\"10\" font-size=\"12\" font-weight=\"400\" letter-spacing=\"0\" {}>{}</tspan>",
                fill(i.muted),
                esc(d)
            ))
            .unwrap_or_default()
    ));
    if let Some(tag) = &v.tag {
        s.push(chip(tag, w - PAD - chip_w(tag), 15., i));
    }
    s.push("</g>".into());
    if !v.bare() {
        s.push(format!(
            "<rect x=\"1\" y=\"{}\" width=\"{}\" height=\"1\" {}/>",
            js(HEAD - 0.5),
            js(w - 2.),
            fill(i.sep)
        ));
    }
    // 행 — 타입은 오른쪽 끝(널 칸이 있으면 그 왼쪽)에 왼쪽 정렬한 한 칸
    let k = if v.show(CardPart::Keys) { KEY } else { 0. };
    let nw = if v.show(CardPart::Nulls) && v.shown.iter().any(|c| is_null(c, t)) {
        null_w()
    } else {
        0.
    };
    let type_x = w - PAD - nw - max_of(v.shown.iter().map(|c| mono_w(&c.ty, 11.)), 40.);
    for (idx, c) in v.shown.iter().enumerate() {
        let cy = row_y(idx);
        let by = cy + 4.2;
        s.push(format!(
            "<g class=\"rz-c\" data-c=\"{}\"><title>{}</title>",
            esc(&c.name),
            esc(&row_title(c, t))
        ));
        s.push(format!(
            "<rect class=\"rz-hit\" x=\"1\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"currentColor\" fill-opacity=\"0\"/>",
            js(f(cy - ROW / 2.)),
            js(w - 2.),
            js(ROW)
        ));
        if idx > 0 {
            s.push(format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"1\" {} opacity=\".7\"/>",
                js(PAD),
                js(f(cy - ROW / 2.)),
                js(w - PAD * 2.),
                fill(i.sep)
            ));
        }
        if let Some((text, ink)) = v.show(CardPart::Keys).then(|| key_label(c, v, i)).flatten() {
            s.push(format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"{MONO}\" font-size=\"8.5\" font-weight=\"600\" {}>{text}</text>",
                js(PAD),
                js(f(by - 0.6)),
                fill(ink)
            ));
        }
        let tags = if v.show(CardPart::Tags) {
            chips(c, v)
        } else {
            Vec::new()
        };
        s.push(format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"12.5\" {}{}>{}{}</text>",
            js(PAD + k),
            js(f(by)),
            fill(i.text),
            if c.pk { " font-weight=\"600\"" } else { "" },
            esc(&c.name),
            tags.iter()
                .enumerate()
                .map(|(j, x)| format!(
                    "<tspan dx=\"{}\" font-family=\"{MONO}\" font-size=\"{}\" font-weight=\"600\" letter-spacing=\"0.04em\" {}>{}</tspan>",
                    if j > 0 { 6 } else { 7 },
                    js(TAG_SIZE),
                    fill(i.faint),
                    esc(x)
                ))
                .collect::<String>()
        ));
        if let Some(d) = c.description.as_deref().filter(|d| !d.is_empty())
            && v.show(CardPart::Descriptions)
        {
            s.push(format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"11.5\" {}>{}</text>",
                js(f(PAD + k + v.name_w + 14.)),
                js(f(by)),
                fill(i.muted),
                esc(d)
            ));
        }
        if v.show(CardPart::Types) {
            s.push(format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"{MONO}\" font-size=\"11\" {}>{}</text>",
                js(f(type_x)),
                js(f(by)),
                fill(i.ty),
                esc(&c.ty)
            ));
        }
        if nw != 0. && is_null(c, t) {
            s.push(chip("NULL", w - PAD - chip_w("NULL"), cy - 7., i));
        }
        s.push("</g>".into());
    }
    if !v.foot.is_empty() {
        let fy = HEAD + 4. + v.shown.len() as f64 * ROW + 4.;
        s.push(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"1\" {}/>",
            js(PAD),
            js(fy),
            js(w - PAD * 2.),
            fill(i.sep)
        ));
        for (k, line) in v.foot.iter().enumerate() {
            s.push(format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"{MONO}\" font-size=\"9.5\" {}>{}</text>",
                js(PAD),
                js(f(fy + 15. + k as f64 * FOOT)),
                fill(i.faint),
                esc(line)
            ));
        }
    }
    s.concat()
}

// ---- 연결선 ----

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pt {
    x: f64,
    y: f64,
}

fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

/// 직각 경로의 꺾임을 반지름 r 로 둥글린다
fn path_d(pts: &[Pt], r: f64) -> String {
    let mut d = format!("M{},{}", js(f(pts[0].x)), js(f(pts[0].y)));
    for i in 1..pts.len().saturating_sub(1) {
        let (p0, p1, p2) = (pts[i - 1], pts[i], pts[i + 1]);
        let d1 = (p1.x - p0.x).hypot(p1.y - p0.y);
        let d2 = (p2.x - p1.x).hypot(p2.y - p1.y);
        let rr = r.min(d1 / 2.).min(d2 / 2.);
        let a = pt(
            p1.x - ((p1.x - p0.x) / d1) * rr,
            p1.y - ((p1.y - p0.y) / d1) * rr,
        );
        let b = pt(
            p1.x + ((p2.x - p1.x) / d2) * rr,
            p1.y + ((p2.y - p1.y) / d2) * rr,
        );
        d += &format!(
            " L{},{} Q{},{} {},{}",
            js(f(a.x)),
            js(f(a.y)),
            js(f(p1.x)),
            js(f(p1.y)),
            js(f(b.x)),
            js(f(b.y))
        );
    }
    let last = pts[pts.len() - 1];
    format!("{d} L{},{}", js(f(last.x)), js(f(last.y)))
}

/// 곡선 연결선 — 사이에 카드가 없으면 끝이 수평인 S자(세로면 수직), 있으면 ELK 길에 넓은 꺾임
fn curved(route: &[Pt], others: &[&SvgBox], down: bool) -> String {
    let (a, z) = (route[0], route[route.len() - 1]);
    let s = if down {
        pt(a.x, a.y + LEAD)
    } else {
        pt(a.x + LEAD, a.y)
    };
    let e = if down {
        pt(z.x, z.y - LEAD)
    } else {
        pt(z.x - LEAD, z.y)
    };
    let gap = if down { e.y - s.y } else { e.x - s.x };
    if gap > 24. {
        let k = 32f64.max(180f64.min(gap * 0.5));
        let c1 = if down {
            pt(s.x, s.y + k)
        } else {
            pt(s.x + k, s.y)
        };
        let c2 = if down {
            pt(e.x, e.y - k)
        } else {
            pt(e.x - k, e.y)
        };
        let blocked = others.iter().any(|b| {
            (1..24).any(|i| {
                let t = i as f64 / 24.;
                let u = 1. - t;
                let x = u * u * u * s.x
                    + 3. * u * u * t * c1.x
                    + 3. * u * t * t * c2.x
                    + t * t * t * e.x;
                let y = u * u * u * s.y
                    + 3. * u * u * t * c1.y
                    + 3. * u * t * t * c2.y
                    + t * t * t * e.y;
                x > b.x - 6. && x < b.x + b.w + 6. && y > b.y - 6. && y < b.y + b.h + 6.
            })
        });
        if !blocked {
            return format!(
                "M{},{} L{},{} C{},{} {},{} {},{} L{},{}",
                js(f(a.x)),
                js(f(a.y)),
                js(f(s.x)),
                js(f(s.y)),
                js(f(c1.x)),
                js(f(c1.y)),
                js(f(c2.x)),
                js(f(c2.y)),
                js(f(e.x)),
                js(f(e.y)),
                js(f(z.x)),
                js(f(z.y))
            );
        }
    }
    path_d(route, 40.)
}

/// 겹친 점과 곧은 길 중간의 점을 뺀다 — 선 한가운데가 둥글려지지 않게
fn clean(pts: &[Pt]) -> Vec<Pt> {
    let same = |a: f64, b: f64| (a - b).abs() < 0.01;
    let mut out: Vec<Pt> = Vec::new();
    for &p in pts {
        if out
            .last()
            .is_none_or(|q| !same(q.x, p.x) || !same(q.y, p.y))
        {
            out.push(p);
        }
    }
    let mut i = out.len().saturating_sub(2);
    while i > 0 {
        let (a, b, c) = (out[i - 1], out[i], out[i + 1]);
        if (same(a.x, b.x) && same(b.x, c.x)) || (same(a.y, b.y) && same(b.y, c.y)) {
            out.remove(i);
        }
        i -= 1;
    }
    out
}

// ---- 모으기 ----

fn views<'a>(
    model: &'a Model,
    columns: Columns,
    audit: AuditView,
    hide: &HashSet<CardPart>,
    rows: bool,
) -> (Vec<View<'a>>, Vec<Relation>) {
    let expand = audit == AuditView::Expand;
    let tables: Vec<&ModelTable> = model
        .tables
        .iter()
        .filter(|t| expand || t.origin != TableOrigin::Audit)
        .collect();
    let names: HashSet<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    let kept: Vec<&Relation> = model
        .relations
        .iter()
        .filter(|r| names.contains(r.parent.as_str()) && names.contains(r.child.as_str()))
        .collect();
    let flat: Vec<Relation> = kept
        .iter()
        .flat_map(|r| match &r.child_columns {
            Some(cs) => cs
                .iter()
                .enumerate()
                .map(|(i, column)| Relation {
                    child_column: column.clone(),
                    parent_column: r.parent_columns.as_ref().expect("짝")[i].clone(),
                    ..(*r).clone()
                })
                .collect(),
            None => vec![(*r).clone()],
        })
        .collect();
    let referenced: HashSet<String> = flat
        .iter()
        .map(|r| format!("{}.{}", r.parent, r.parent_column))
        .collect();
    let mut child_columns: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in &flat {
        child_columns
            .entry(r.child.as_str())
            .or_default()
            .push(r.child_column.as_str());
    }
    // 행을 잇는다(가로) — 복합 참조는 컬럼 짝마다 한 선. 카드를 잇는다(세로) — 한 선
    let relations: Vec<Relation> = if rows {
        flat.clone()
    } else {
        kept.iter().map(|r| (*r).clone()).collect()
    };

    let mut out = Vec::new();
    for table in tables {
        // 대상이 그려지지 않아도 참조를 든 컬럼은 FK 다
        let mut ref_cols: HashSet<String> = child_columns
            .get(table.name.as_str())
            .into_iter()
            .flatten()
            .map(|s| s.to_string())
            .collect();
        ref_cols.extend(
            table
                .columns
                .iter()
                .filter(|c| c.reference.is_some())
                .map(|c| c.name.clone()),
        );
        ref_cols.extend(
            table
                .foreign_keys
                .iter()
                .flatten()
                .flat_map(|k| k.columns.iter().cloned()),
        );
        let is_key = |c: &ModelColumn| {
            c.pk || c.uk
                || ref_cols.contains(&c.name)
                || referenced.contains(&format!("{}.{}", table.name, c.name))
        };
        let shown: Vec<&ModelColumn> = match columns {
            Columns::None => Vec::new(),
            Columns::Keys => table.columns.iter().filter(|c| is_key(c)).collect(),
            Columns::All => table.columns.iter().collect(),
        };
        let hidden = table.columns.len() - shown.len();
        let tag = if table.origin == TableOrigin::External {
            Some("EXTERNAL".to_string())
        } else if table.origin == TableOrigin::Audit || (table.audit.is_some() && !expand) {
            Some(
                table
                    .audit
                    .as_ref()
                    .map_or("envers", |a| a.method.as_str())
                    .to_uppercase(),
            )
        } else {
            None
        };
        // 이름만 보기는 머리 아래를 다 뺀다 — 제약도, 컬럼 수도
        let mut foot: Vec<String> = Vec::new();
        if columns != Columns::None {
            foot.extend(table.constraints.iter().map(|k| {
                format!(
                    "{} ({})",
                    k.name
                        .clone()
                        .unwrap_or_else(|| if k.kind == super::ConstraintKind::Unique {
                            "unique"
                        } else {
                            "index"
                        }
                        .into()),
                    k.columns.join(", ")
                )
            }));
            for fk in table.foreign_keys.iter().flatten() {
                foot.push(format!(
                    "{} ({})",
                    fk.name.clone().unwrap_or_else(|| "foreign".into()),
                    fk.columns.join(", ")
                ));
            }
            if hidden > 0 {
                foot.push(format!(
                    "+{hidden} {}",
                    if hidden == 1 { "column" } else { "columns" }
                ));
            }
        }
        let mut v = View {
            table,
            shown,
            ref_cols,
            tag,
            foot,
            hide: hide.clone(),
            w: 0.,
            h: 0.,
            name_w: 0.,
        };
        measure(&mut v);
        out.push(v);
    }
    (out, relations)
}

const STANDALONE_STYLE: &str =
    "<style>.rz{color:#1f2328}@media (prefers-color-scheme:dark){.rz{color:#e6edf3}}</style>";

fn rgb01(hex: &str) -> [f64; 3] {
    [1, 3, 5].map(|i| f3(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f64 / 255.))
}

fn card_defs(l: &Look, id: &dyn Fn(&str) -> String) -> String {
    let mut s = String::new();
    match l.glass {
        Some(g) => {
            s += &format!(
                "<linearGradient id=\"{}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">{}{}</linearGradient>",
                id("wash"),
                stop(0., g.tint),
                stop(1., (g.tint.0, 0.))
            )
        }
        None => {
            s += &format!(
                "<linearGradient id=\"{}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">{}{}</linearGradient>",
                id("wash"),
                stop(0., (C, 0.07)),
                stop(1., (C, 0.025))
            )
        }
    }
    // 그림자 둘(넓게 깔리는 것, 붙은 것) 뒤 카드 모양을 도려낸다 — 반투명 유리로 제 그림자가 비치지 않게
    let (sc, sa) = l.glass.map_or(("#000000", 0.16), |g| g.shadow);
    s += &format!(
        "<filter id=\"{}\" x=\"-30%\" y=\"-30%\" width=\"160%\" height=\"180%\"><feGaussianBlur in=\"SourceAlpha\" stdDeviation=\"18\"/><feOffset dy=\"16\" result=\"a\"/><feFlood flood-color=\"{sc}\" flood-opacity=\"{}\"/><feComposite in2=\"a\" operator=\"in\" result=\"ambient\"/><feGaussianBlur in=\"SourceAlpha\" stdDeviation=\"1.5\"/><feOffset dy=\"1\" result=\"c\"/><feFlood flood-color=\"{sc}\" flood-opacity=\"{}\"/><feComposite in2=\"c\" operator=\"in\" result=\"contact\"/><feMerge><feMergeNode in=\"ambient\"/><feMergeNode in=\"contact\"/></feMerge><feComposite in2=\"SourceAlpha\" operator=\"out\"/></filter>",
        id("shadow"),
        js(sa),
        js(f3(sa * 0.7))
    );
    s
}

/// 고정 난수(mulberry32) — 그린 하늘이 매번 같게
fn random(seed: u32) -> impl FnMut() -> f64 {
    let mut s = seed as i32;
    move || {
        s = s.wrapping_add(0x6d2b79f5u32 as i32);
        let mut t = (s ^ ((s as u32) >> 15) as i32).wrapping_mul(s | 1);
        t ^= t.wrapping_add((t ^ ((t as u32) >> 7) as i32).wrapping_mul(t | 61));
        ((t ^ ((t as u32) >> 14) as i32) as u32) as f64 / 4294967296.
    }
}

/// 그린 오로라 — 빛의 띠 둘(흔들리는 밝은 가장자리와 그 위로 오르는 빛)을 함께 흐리고, 별 하늘
fn aurora_svg(st: &Stage, id: &dyn Fn(&str) -> String, w: f64, h: f64) -> (String, String) {
    let mut d = Vec::new();
    let mut b = Vec::new();
    struct Ribbon {
        ink: (&'static str, f64),
        y: f64,
        amp: f64,
        freq: f64,
        phase: f64,
        rise: f64,
    }
    let ribbons = [
        Ribbon {
            ink: st.colors[1],
            y: 0.46,
            amp: 0.08,
            freq: 1.2,
            phase: 0.6,
            rise: 0.32,
        },
        Ribbon {
            ink: (st.colors[0].0, st.colors[0].1 * 0.8),
            y: 0.28,
            amp: 0.07,
            freq: 0.8,
            phase: 2.1,
            rise: 0.24,
        },
    ];
    d.push(format!(
        "<filter id=\"{}\" x=\"-10%\" y=\"-30%\" width=\"120%\" height=\"160%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
        id("ribbons"),
        js(f(w.max(h) / 110.))
    ));
    for (i, r) in ribbons.iter().enumerate() {
        let (color, k) = r.ink;
        d.push(format!(
            "<linearGradient id=\"{}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">{}{}{}</linearGradient>",
            id(&format!("ribbon{i}")),
            stop(0., (color, 0.)),
            stop(0.75, (color, f3(k * 0.3))),
            stop(1., (color, f3(k * 0.55)))
        ));
        let mut edge = Vec::new();
        let mut top = Vec::new();
        let mut x = -48.;
        while x <= w + 48. {
            let u = (x / w) * std::f64::consts::PI * 2.;
            let y = h * (r.y + r.amp * (u * r.freq + r.phase).sin());
            let rise = h * r.rise * (0.75 + 0.25 * (u * r.freq * 2.3 + r.phase * 1.7).sin());
            edge.push(format!("{},{}", js(f(x)), js(f(y))));
            top.push(format!("{},{}", js(f(x)), js(f(y - rise))));
            x += 24.;
        }
        let poly: Vec<String> = top
            .iter()
            .cloned()
            .chain(edge.iter().rev().cloned())
            .collect();
        b.push(format!(
            "<polygon points=\"{}\" fill=\"url(#{})\"/>",
            poly.join(" "),
            id(&format!("ribbon{i}"))
        ));
        b.push(format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{}\" stroke-width=\"{}\"/>",
            edge.join(" "),
            js(k),
            js(f(h / 90.))
        ));
    }
    let mut sky = Vec::new();
    if let Some((sc, sk)) = st.stars {
        let mut rand = random(7);
        let mut y = 0.;
        while y < h {
            let mut x = 0.;
            while x < w {
                if rand() > 0.2 {
                    x += 28.;
                    continue;
                }
                let sx = x + 6. + rand() * 16.;
                let sy = y + 6. + rand() * 16.;
                let bright = rand();
                sky.push(format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{sc}\" fill-opacity=\"{}\"/>",
                    js(f(sx)),
                    js(f(sy)),
                    js(f(0.45 + bright * 0.8)),
                    js(f3(sk * (0.3 + 0.7 * bright)))
                ));
                if bright > 0.9 {
                    sky.push(format!(
                        "<path d=\"M{},{}H{}M{},{}V{}\" stroke=\"{sc}\" stroke-opacity=\"{}\" stroke-width=\"0.6\"/>",
                        js(f(sx - 5.)),
                        js(f(sy)),
                        js(f(sx + 5.)),
                        js(f(sx)),
                        js(f(sy - 5.)),
                        js(f(sy + 5.)),
                        js(f3(sk * 0.35))
                    ));
                }
                x += 28.;
            }
            y += 28.;
        }
    }
    (
        d.concat(),
        format!(
            "<g filter=\"url(#{})\">{}</g>{}",
            id("ribbons"),
            b.concat(),
            sky.concat()
        ),
    )
}

/// 파일의 무대 — 바탕과 멈춘 빛, 점 격자, 결, 비네트. 유리는 그 사본(격자, 결 없이)을 카드마다 흐려 비춘다
fn stage_svg(
    st: &Stage,
    g: &Glass,
    id: &dyn Fn(&str) -> String,
    w: f64,
    h: f64,
    cards: &str,
) -> (String, String, String) {
    let mut d = Vec::new();
    let r_ = w.max(h);
    let mut light = vec![format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
        js(w),
        js(h),
        st.base
    )];
    // 오로라 띠 아래의 색 덩어리는 희미한 빛일 뿐
    let field = if st.style == "aurora" { 0.35 } else { 1. };
    for (i, (color, strength)) in st.colors.iter().enumerate() {
        let (cx, cy, r) = st.spots[i];
        d.push(format!(
            "<radialGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\"><stop offset=\"0\" stop-color=\"{color}\" stop-opacity=\"{}\"/><stop offset=\"1\" stop-color=\"{color}\" stop-opacity=\"0\"/></radialGradient>",
            id(&format!("spot{i}")),
            js(f(cx * w)),
            js(f(cy * h)),
            js(f(r * r_)),
            js(f3(strength * field))
        ));
        light.push(format!(
            "<rect width=\"{}\" height=\"{}\" fill=\"url(#{})\"/>",
            js(w),
            js(h),
            id(&format!("spot{i}"))
        ));
    }
    if st.style == "aurora" {
        let (defs, body) = aurora_svg(st, id, w, h);
        d.push(defs);
        light.push(body);
    }
    d.push(format!(
        "<g id=\"{}\">{}</g>",
        id("backdrop"),
        light.concat()
    ));
    let step = GRID_STEP;
    d.push(format!(
        "<pattern id=\"{}\" width=\"{}\" height=\"{}\" x=\"{}\" y=\"{}\" patternUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"1.2\" height=\"1.2\" {}/></pattern>",
        id("grid"),
        js(step),
        js(step),
        js(-step / 2.),
        js(-step / 2.),
        js(step / 2. - 0.6),
        js(step / 2. - 0.6),
        fill(st.grid)
    ));
    let [gr, gg, gb] = rgb01(if st.dark { WHITE } else { "#000000" });
    d.push(format!(
        "<filter id=\"{}\" x=\"0\" y=\"0\" width=\"100%\" height=\"100%\" color-interpolation-filters=\"sRGB\"><feTurbulence type=\"fractalNoise\" baseFrequency=\"1.1\" numOctaves=\"2\" seed=\"7\" stitchTiles=\"stitch\"/><feColorMatrix type=\"matrix\" values=\"0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} {} 0 0 0 {}\"/></filter>",
        id("grain"),
        js(gr),
        js(gg),
        js(gb),
        js(f3(st.grain * 4.)),
        js(f3(-st.grain * 1.6))
    ));
    d.push(format!(
        "<radialGradient id=\"{}\" cx=\"0.5\" cy=\"0.45\" r=\"0.75\"><stop offset=\"0.55\" stop-color=\"#000000\" stop-opacity=\"0\"/><stop offset=\"1\" stop-color=\"#000000\" stop-opacity=\"{}\"/></radialGradient>",
        id("vignette"),
        js(st.vignette)
    ));
    d.push(format!(
        "<filter id=\"{}\" x=\"0\" y=\"0\" width=\"100%\" height=\"100%\" color-interpolation-filters=\"sRGB\"><feGaussianBlur stdDeviation=\"{}\" edgeMode=\"duplicate\"/><feColorMatrix type=\"saturate\" values=\"1.35\"/></filter>",
        id("frost"),
        js(g.frost)
    ));
    d.push(format!(
        "<mask id=\"{}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"><rect width=\"{}\" height=\"{}\" fill=\"black\"/>{cards}</mask>",
        id("cards"),
        js(w),
        js(h),
        js(w),
        js(h)
    ));
    let under = format!(
        "<g class=\"rz-stage\"><use href=\"#{}\"/><rect class=\"rz-grid\" width=\"{}\" height=\"{}\" fill=\"url(#{})\"/><rect width=\"{}\" height=\"{}\" fill=\"#000000\" filter=\"url(#{})\"/><rect width=\"{}\" height=\"{}\" fill=\"url(#{})\"/></g>",
        id("backdrop"),
        js(w),
        js(h),
        id("grid"),
        js(w),
        js(h),
        id("grain"),
        js(w),
        js(h),
        id("vignette")
    );
    let frost = format!(
        "<g mask=\"url(#{})\"><use href=\"#{}\" filter=\"url(#{})\"/></g>",
        id("cards"),
        id("backdrop"),
        id("frost")
    );
    (d.concat(), under, frost)
}

// ---- 서비스 ----

/// 서비스 노드의 ELK id — 표 이름은 NUL 로 시작하지 못한다
const SERVICE: &str = "\u{0}service:";
const SERVICE_PAD: f64 = 24.;
const SERVICE_HEAD: f64 = 52.;
const SERVICE_RX: f64 = 12.;
const HUES_DARK: [&str; 6] = [
    "#5EEAD4", "#C4B5FD", "#FCD34D", "#FDA4AF", "#7DD3FC", "#BEF264",
];
const HUES_LIGHT: [&str; 6] = [
    "#0F766E", "#6D28D9", "#B45309", "#BE123C", "#0369A1", "#4D7C0F",
];

fn service_hue(l: &Look, index: usize) -> Option<&'static str> {
    let hues = l
        .stage
        .as_ref()
        .map(|s| if s.dark { HUES_DARK } else { HUES_LIGHT })?;
    Some(hues[index % hues.len()])
}
fn service_edge(l: &Look) -> f64 {
    if l.stage.as_ref().is_some_and(|s| s.dark) {
        0.42
    } else {
        0.38
    }
}
fn service_label_w(sv: &ModelService) -> f64 {
    sans_w(&sv.name, 13.) * 1.04
        + sv.description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map_or(0., |d| 10. + sans_w(d, 12.))
}

struct Area<'a> {
    service: &'a ModelService,
    hue: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// 카드 밑의 영역 — 옅은 칠, 고른 테, 왼쪽 위 이름. 칠은 카드 자리를 도려낸다(라이브 캔버스는 유리를 이 SVG 아래에 칠한다)
fn service_areas(
    areas: &[Area],
    boxes: &[SvgBox],
    l: &Look,
    id: &dyn Fn(&str) -> String,
    down: bool,
) -> String {
    let mut s = Vec::new();
    let holes: String = boxes
        .iter()
        .map(|b| {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"black\"/>",
                js(b.x),
                js(b.y),
                js(b.w),
                js(b.h),
                js(RX)
            )
        })
        .collect();
    s.push(format!(
        "<mask id=\"{}\" maskUnits=\"userSpaceOnUse\"><rect x=\"-1e5\" y=\"-1e5\" width=\"2e5\" height=\"2e5\" fill=\"white\"/>{holes}</mask>",
        id("areas")
    ));
    // 세로면 다른 서비스에서 오는 선이 이름이 있는 위쪽으로 들어온다 — 이름을 지나지 말고 그 뒤에서 끊긴다
    if down {
        let names: String = areas
            .iter()
            .map(|a| {
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"26\" fill=\"black\"/>",
                    js(f(a.x + SERVICE_PAD - 8.)),
                    js(f(a.y + 13.)),
                    js(f(service_label_w(a.service) + 16.))
                )
            })
            .collect();
        s.push(format!(
            "<mask id=\"{}\" maskUnits=\"userSpaceOnUse\"><rect x=\"-1e5\" y=\"-1e5\" width=\"2e5\" height=\"2e5\" fill=\"white\"/>{names}</mask>",
            id("names")
        ));
    }
    s.push("<g class=\"rz-services\">".into());
    for a in areas {
        let hue = service_hue(l, a.hue);
        let (fill_a, line_a) = if hue.is_some() {
            (0.07, service_edge(l))
        } else {
            (0.035, 0.22)
        };
        let color = hue.unwrap_or(C);
        s.push(format!(
            "<g class=\"rz-svc\" data-svc=\"{}\">",
            esc(&a.service.name)
        ));
        s.push(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{color}\" fill-opacity=\"{}\" mask=\"url(#{})\"/>",
            js(a.x), js(a.y), js(a.w), js(a.h), js(SERVICE_RX), js(fill_a), id("areas")
        ));
        s.push(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{}\"/>",
            js(f(a.x + 0.5)), js(f(a.y + 0.5)), js(f(a.w - 1.)), js(f(a.h - 1.)), js(SERVICE_RX - 0.5), js(line_a)
        ));
        s.push(format!(
            "<g class=\"rz-svc-head\"><rect class=\"rz-hit\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"26\" rx=\"4\" fill=\"currentColor\" fill-opacity=\"0\"/>",
            js(f(a.x + SERVICE_PAD - 8.)),
            js(f(a.y + 13.)),
            js(f(service_label_w(a.service) + 16.))
        ));
        s.push(format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"13\" font-weight=\"600\" letter-spacing=\"-0.01em\" {}>{}{}</text>",
            js(f(a.x + SERVICE_PAD)),
            js(f(a.y + 31.)),
            match hue {
                Some(hh) => format!("fill=\"{hh}\""),
                None => fill(l.ink.text),
            },
            esc(&a.service.name),
            a.service
                .description
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| format!(
                    "<tspan dx=\"10\" font-size=\"12\" font-weight=\"400\" letter-spacing=\"0\" {}>{}</tspan>",
                    fill(l.ink.muted),
                    esc(d)
                ))
                .unwrap_or_default()
        ));
        s.push("</g></g>".into());
    }
    s.push("</g>".into());
    s.concat()
}

fn opts_map(pairs: &[(&str, String)]) -> Json {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), json!(v));
    }
    Json::Object(m)
}

/// 모델을 SVG 로 — 배치는 ELK(elk.rs)
pub fn to_svg(model: &Model, options: &SvgOptions) -> Result<SvgResult, String> {
    to_svg_with(model, options, &|g| crate::elk::layout(g))
}

/// 배치를 받아 그린다 — 시험은 기록한 배치를 넘길 수 있다
pub fn to_svg_with(
    model: &Model,
    options: &SvgOptions,
    layout: &dyn Fn(&str) -> Result<String, String>,
) -> Result<SvgResult, String> {
    let hide: HashSet<CardPart> = options.hide.iter().copied().collect();
    let look = options.look.as_str();
    let edges = options.edges;
    let down = options.direction == Direction::Down;
    let l = look_of(look);
    let prefix = options.id_prefix.clone().unwrap_or_else(|| "rz-".into());
    let id = move |name: &str| format!("{prefix}{name}");
    let (vs, relations) = views(model, options.columns, options.audit, &hide, !down);
    let by_name: HashMap<&str, usize> = vs
        .iter()
        .enumerate()
        .map(|(i, v)| (v.table.name.as_str(), i))
        .collect();
    let port_id = |table: &str, column: &str, side: &str| format!("{table}::{column}::{side}");
    let port_for = |table: &str, column: &str, side: &str| {
        port_id(table, port_of(&vs[by_name[table]], column), side)
    };
    let used: HashSet<String> = relations
        .iter()
        .flat_map(|r| {
            [
                port_for(&r.parent, &r.parent_column, "E"),
                port_for(&r.child, &r.child_column, "W"),
            ]
        })
        .collect();
    // 한 기본 키 행에서 나가는 선들은 첫 구간을 함께 쓴다. NOT NULL 과 널 둘 다 가리키면 종류마다 따로
    let mut optional: HashMap<String, HashSet<bool>> = HashMap::new();
    for r in &relations {
        optional
            .entry(port_for(&r.parent, &r.parent_column, "E"))
            .or_default()
            .insert(r.optional);
    }
    let apart = |r: &Relation| -> f64 {
        if !down && optional[&port_for(&r.parent, &r.parent_column, "E")].len() > 1 {
            if r.optional { SPLIT } else { -SPLIT }
        } else {
            0.
        }
    };
    // 세로 — 끝마다 제 포트, ELK 가 가장자리를 따라 벌린다. id 는 NUL 로 시작한다
    let end_id = |table: &str, i: usize, side: &str| format!("\u{0}{table}::e{i}::{side}");
    let card_ports = |v: &View| -> Vec<Json> {
        relations
            .iter()
            .enumerate()
            .flat_map(|(i, r)| {
                let mut out = Vec::new();
                if r.parent == v.table.name {
                    out.push(json!({"id": end_id(&r.parent, i, "S"), "x": num(v.w / 2.), "y": num(v.h), "width": 0, "height": 0, "layoutOptions": {"elk.port.side": "SOUTH"}}));
                }
                if r.child == v.table.name {
                    out.push(json!({"id": end_id(&r.child, i, "N"), "x": num(v.w / 2.), "y": 0, "width": 0, "height": 0, "layoutOptions": {"elk.port.side": "NORTH"}}));
                }
                out
            })
            .collect()
    };
    let table_node = |v: &View| -> Json {
        let ports: Vec<Json> = if down {
            card_ports(v)
        } else {
            let rows: Vec<(String, f64)> = if v.bare() {
                vec![(String::new(), HEAD / 2.)]
            } else {
                v.shown
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.name.clone(), row_y(i)))
                    .collect()
            };
            rows.iter()
                .flat_map(|(name, y)| {
                    [("W", 0., "WEST"), ("E", v.w, "EAST")]
                        .into_iter()
                        .filter(|(side, _, _)| used.contains(&port_id(&v.table.name, name, side)))
                        .map(|(side, x, elk_side)| json!({"id": port_id(&v.table.name, name, side), "x": num(x), "y": num(*y), "width": 0, "height": 0, "layoutOptions": {"elk.port.side": elk_side}}))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        json!({
            "id": v.table.name,
            "width": num(v.w),
            "height": num(v.h),
            "layoutOptions": {"elk.portConstraints": if down { "FIXED_SIDE" } else { "FIXED_POS" }},
            "ports": ports,
        })
    };
    // 그려지는 표가 있는 서비스는 제 노드가 되어 그 표들을 품는다 — 첫 표가 나오는 자리에
    let drawn_services: Vec<&ModelService> = model
        .services
        .iter()
        .filter(|sv| {
            vs.iter()
                .any(|v| v.table.service.as_deref() == Some(sv.name.as_str()))
        })
        .collect();
    let service_of = |v: &View| -> Option<&ModelService> {
        drawn_services
            .iter()
            .copied()
            .find(|sv| v.table.service.as_deref() == Some(sv.name.as_str()))
    };
    let mut children: Vec<Json> = Vec::new();
    for v in &vs {
        match service_of(v) {
            None => children.push(table_node(v)),
            Some(sv) => {
                let sid = format!("{SERVICE}{}", sv.name);
                if children.iter().any(|n| n["id"] == json!(sid)) {
                    continue;
                }
                let members: Vec<Json> = vs
                    .iter()
                    .filter(|x| service_of(x).is_some_and(|s| std::ptr::eq(s, sv)))
                    .map(&table_node)
                    .collect();
                children.push(json!({
                    "id": sid,
                    "layoutOptions": opts_map(&[
                        ("elk.padding", format!("[top={},left={},bottom={},right={}]", js(SERVICE_HEAD), js(SERVICE_PAD), js(SERVICE_PAD), js(SERVICE_PAD))),
                        ("elk.nodeSize.constraints", "MINIMUM_SIZE".into()),
                        ("elk.nodeSize.minimum", format!("({}, 0)", js((service_label_w(sv) + SERVICE_PAD * 2.).ceil()))),
                        ("elk.layered.spacing.nodeNodeBetweenLayers", "48".into()),
                        ("elk.layered.spacing.edgeNodeBetweenLayers", "36".into()),
                        ("elk.spacing.nodeSelfLoop", "36".into()),
                    ]),
                    "children": members,
                }));
            }
        }
    }
    let mut degree: HashMap<&str, usize> = HashMap::new();
    for r in &relations {
        for n in [&r.parent, &r.child] {
            *degree.entry(n.as_str()).or_default() += 1;
        }
    }
    let has_hub = degree.values().any(|&n| n >= 32);
    let mut root_opts: Vec<(&str, String)> = Vec::new();
    if !drawn_services.is_empty() {
        root_opts.push(("elk.hierarchyHandling", "INCLUDE_CHILDREN".into()));
    }
    if has_hub {
        root_opts.push(("elk.layered.highDegreeNodes.treatment", "true".into()));
        root_opts.push(("elk.layered.highDegreeNodes.threshold", "32".into()));
    }
    root_opts.extend([
        ("elk.algorithm", "layered".to_string()),
        ("elk.direction", if down { "DOWN" } else { "RIGHT" }.into()),
        ("elk.edgeRouting", "ORTHOGONAL".into()),
        ("elk.layered.spacing.nodeNodeBetweenLayers", "88".into()),
        ("elk.layered.spacing.edgeNodeBetweenLayers", "36".into()),
        ("elk.spacing.nodeSelfLoop", "36".into()),
        ("elk.spacing.nodeNode", "40".into()),
        ("elk.spacing.edgeEdge", "14".into()),
        ("elk.spacing.edgeNode", "24".into()),
        ("elk.spacing.componentComponent", "56".into()),
        (
            "elk.layered.considerModelOrder.strategy",
            "NODES_AND_EDGES".into(),
        ),
        ("elk.layered.nodePlacement.strategy", "BRANDES_KOEPF".into()),
        ("elk.padding", "[top=40,left=40,bottom=40,right=40]".into()),
    ]);
    let graph = json!({
        "id": "root",
        "layoutOptions": opts_map(&root_opts),
        "children": children,
        "edges": relations.iter().enumerate().map(|(i, r)| json!({
            "id": format!("e{i}"),
            "sources": [if down { end_id(&r.parent, i, "S") } else { port_for(&r.parent, &r.parent_column, "E") }],
            "targets": [if down { end_id(&r.child, i, "N") } else { port_for(&r.child, &r.child_column, "W") }],
        })).collect::<Vec<_>>(),
    });
    let g: Json = serde_json::from_str(&layout(&graph.to_string())?).map_err(|e| e.to_string())?;

    // 서비스 안의 표와 그 안에 머무는 선은 서비스 기준 자리다
    let n64 = |v: &Json, k: &str| v.get(k).and_then(Json::as_f64).unwrap_or(0.);
    let mut pos: HashMap<String, Pt> = HashMap::new();
    let mut offsets: HashMap<String, Pt> = HashMap::from([("root".to_string(), pt(0., 0.))]);
    let mut areas: Vec<Area> = Vec::new();
    let nodes = g["children"].as_array().cloned().unwrap_or_default();
    for n in &nodes {
        let nid = n["id"].as_str().unwrap_or_default();
        let at = pt(n64(n, "x"), n64(n, "y"));
        if !nid.starts_with(SERVICE) {
            pos.insert(nid.to_string(), at);
            continue;
        }
        offsets.insert(nid.to_string(), at);
        let sv = drawn_services
            .iter()
            .copied()
            .find(|x| format!("{SERVICE}{}", x.name) == nid)
            .expect("그린 서비스");
        areas.push(Area {
            service: sv,
            hue: model
                .services
                .iter()
                .position(|x| std::ptr::eq(x, sv))
                .unwrap_or(0),
            x: f(at.x),
            y: f(at.y),
            w: f(n64(n, "width")),
            h: f(n64(n, "height")),
        });
        for c in n["children"].as_array().into_iter().flatten() {
            pos.insert(
                c["id"].as_str().unwrap_or_default().to_string(),
                pt(at.x + n64(c, "x"), at.y + n64(c, "y")),
            );
        }
    }
    let mut routes: HashMap<String, (Option<Json>, Pt)> = HashMap::new();
    let all_edges = g["edges"].as_array().into_iter().flatten().chain(
        nodes
            .iter()
            .flat_map(|n| n["edges"].as_array().into_iter().flatten()),
    );
    for e in all_edges {
        let container = e.get("container").and_then(Json::as_str).unwrap_or("root");
        let offset = offsets.get(container).copied().unwrap_or(pt(0., 0.));
        routes.insert(
            e["id"].as_str().unwrap_or_default().to_string(),
            (e.get("sections").and_then(|s| s.get(0)).cloned(), offset),
        );
    }
    let w_ = n64(&g, "width").ceil();
    let h_ = n64(&g, "height").ceil();
    let boxes: Vec<SvgBox> = vs
        .iter()
        .map(|v| {
            let p = pos[&v.table.name];
            SvgBox {
                table: v.table.name.clone(),
                x: f(p.x),
                y: f(p.y),
                w: v.w,
                h: v.h,
            }
        })
        .collect();
    // 라이브 캔버스는 무대와 판을 제가 칠한다 — SVG 엔 그 위의 것만
    let painted = !(l.stage.is_some() && options.stage == Some(false));
    let card_rects: String = boxes
        .iter()
        .map(|b| {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"white\"/>",
                js(b.x),
                js(b.y),
                js(b.w),
                js(b.h),
                js(RX)
            )
        })
        .collect();
    let stage = match (&l.stage, l.glass) {
        (Some(st), Some(gl)) if painted => Some(stage_svg(st, gl, &id, w_, h_, &card_rects)),
        _ => None,
    };

    let mut s: Vec<String> = Vec::new();
    let label = format!(
        "ERD: {}",
        vs.iter()
            .map(|v| v.table.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    s.push(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" class=\"rz rz-{look}\" role=\"img\" aria-label=\"{}\" font-family=\"{SANS}\" font-size=\"12\">",
        esc(&label),
        w = js(w_),
        h = js(h_)
    ));
    if options.standalone && l.stage.is_none() {
        s.push(STANDALONE_STYLE.into());
    }
    s.push(format!("<title>{}</title>", esc(&label)));
    s.push(format!(
        "<defs>{}{}</defs>",
        card_defs(&l, &id),
        stage.as_ref().map_or("", |x| x.0.as_str())
    ));
    if let Some((_, under, _)) = &stage {
        s.push(under.clone());
    }
    if painted {
        // 그림자 먼저, 유리 아래에. data-t 로 뷰어가 카드와 함께 흐리게 한다
        s.push("<g class=\"rz-shadows\">".into());
        for b in &boxes {
            s.push(format!(
                "<rect class=\"rz-s\" data-t=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"currentColor\" filter=\"url(#{})\"/>",
                esc(&b.table), js(b.x), js(b.y), js(b.w), js(b.h), js(RX), id("shadow")
            ));
        }
        s.push("</g>".into());
    }
    if let Some((_, _, frost)) = &stage {
        s.push(frost.clone());
    }
    if !areas.is_empty() {
        s.push(service_areas(&areas, &boxes, &l, &id, down));
    }
    s.push(if down && !areas.is_empty() {
        format!("<g class=\"rz-rels\" mask=\"url(#{})\">", id("names"))
    } else {
        "<g class=\"rz-rels\">".into()
    });
    for (i, r) in relations.iter().enumerate() {
        let (sec, offset) = routes
            .get(&format!("e{i}"))
            .cloned()
            .unwrap_or((None, pt(0., 0.)));
        let at = |p: &Json| pt(n64(p, "x") + offset.x, n64(p, "y") + offset.y);
        let bends: Vec<Pt> = sec
            .as_ref()
            .and_then(|s| s.get("bendPoints"))
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .map(at)
            .collect();
        let (pv, cv) = (
            &vs[by_name[r.parent.as_str()]],
            &vs[by_name[r.child.as_str()]],
        );
        let (pp, cp) = (pos[&r.parent], pos[&r.child]);
        let dy = apart(r);
        // 가로는 끝점을 ELK 좌표가 아니라 행 자리에 붙인다. 세로는 ELK 가 가장자리에 벌린 끝
        let start = if down {
            match &sec {
                Some(sx) => at(&sx["startPoint"]),
                None => pt(pp.x + pv.w / 2., pp.y + pv.h),
            }
        } else {
            pt(pp.x + pv.w, pp.y + anchor_y(pv, &r.parent_column) + dy)
        };
        let end = if down {
            match &sec {
                Some(sx) => at(&sx["endPoint"]),
                None => pt(cp.x + cv.w / 2., cp.y),
            }
        } else {
            pt(cp.x, cp.y + anchor_y(cv, &r.child_column))
        };
        let mut path = clean(&[vec![start], bends, vec![end]].concat());
        // 벌린 첫 구간은 수평을 지킨다 — 닿는 모서리가 함께 움직이거나, 곧은 선이면 계단으로 제 행에 돌아온다
        if dy != 0. && path.len() > 2 {
            path[1] = pt(path[1].x, path[1].y + dy);
        } else if dy != 0. {
            path.splice(
                1..1,
                [pt(start.x + 24., start.y), pt(start.x + 24., start.y - dy)],
            );
        }
        let out = if down { pt(0., 1.) } else { pt(1., 0.) };
        let back = pt(-out.x, -out.y);
        let along = |p: Pt, u: Pt, k: f64, side: f64| {
            pt(p.x + u.x * k - u.y * side, p.y + u.y * k + u.x * side)
        };
        let xy = |p: Pt| format!("{},{}", js(f(p.x)), js(f(p.y)));
        let bar = |p: Pt, u: Pt, k: f64| {
            format!("M{} L{}", xy(along(p, u, k, 4.5)), xy(along(p, u, k, -4.5)))
        };
        // 널이 되는 컬럼의 고리는 선 위에 앉는다 — 자리가 있으면 선이 고리 둘레에서 끊긴다
        let gap =
            r.optional && path.len() > 1 && (path[1].x - start.x).hypot(path[1].y - start.y) > 22.;
        let rest: Vec<Pt> = if gap {
            [vec![along(start, out, 15.5, 0.)], path[1..].to_vec()].concat()
        } else {
            path.clone()
        };
        let line = if edges == Edges::Curved {
            let others: Vec<&SvgBox> = boxes
                .iter()
                .filter(|b| b.table != r.parent && b.table != r.child)
                .collect();
            curved(&rest, &others, down)
        } else {
            path_d(&rest, 3.)
        };
        let d = if gap {
            format!("M{} L{} {line}", xy(start), xy(along(start, out, 8.5, 0.)))
        } else {
            line
        };
        let dash = if r.kind == RefKind::Logical {
            " stroke-dasharray=\"4 3\""
        } else {
            ""
        };
        let ink = l.ink.line;
        let composite = match (&r.parent_columns, &r.child_columns) {
            (Some(pc), Some(cc)) => format!(
                " data-acs=\"{}\" data-bcs=\"{}\"",
                esc(&serde_json::to_string(pc).unwrap_or_default()),
                esc(&serde_json::to_string(cc).unwrap_or_default())
            ),
            _ => String::new(),
        };
        s.push(format!(
            "<g class=\"rz-r\" data-a=\"{}\" data-ac=\"{}\" data-b=\"{}\" data-bc=\"{}\"{composite}>",
            esc(&r.parent),
            esc(&r.parent_column),
            esc(&r.child),
            esc(&r.child_column)
        ));
        s.push(format!(
            "<path d=\"{d}\" fill=\"none\" {} stroke-width=\"1.2\"{dash}/>",
            stroke(ink)
        ));
        // 까마귀 발과 곁의 수 — 기본 키 끝: 하나면 막대 둘, 없을 수 있으면 막대와 고리. FK 끝: 여럿이면 발, 하나면 막대
        let mark = format!("fill=\"none\" {} stroke-width=\"1.3\"", stroke(ink));
        let number = |p: Pt, text: &str, anchor: &str| {
            format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" font-family=\"{MONO}\" font-size=\"9.5\" font-weight=\"500\" {}>{text}</text>",
                js(f(p.x)),
                js(f(p.y)),
                fill(ink)
            )
        };
        s.push(format!(
            "<path d=\"{}{}\" {mark}/>",
            bar(start, out, 5.),
            if r.optional {
                String::new()
            } else {
                format!(" {}", bar(start, out, 9.))
            }
        ));
        if r.optional {
            let c = along(start, out, 12., 0.);
            s.push(format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"3.5\" {mark}/>",
                js(f(c.x)),
                js(f(c.y))
            ));
        }
        // 행의 두 첫 구간 중 아래 것은 수를 선 아래에 — 위 것과 겹치지 않게
        let under = dy > 0.;
        s.push(number(
            if down {
                pt(start.x + 7., start.y + 16.)
            } else {
                pt(start.x + 6., start.y + if under { 15. } else { -8. })
            },
            if r.optional { "0..1" } else { "1" },
            "start",
        ));
        s.push(format!(
            "<path d=\"{}\" {mark}/>",
            if r.one {
                bar(end, back, 7.)
            } else {
                format!(
                    "M{} L{} L{}",
                    xy(along(end, back, 0., 6.)),
                    xy(along(end, back, 12., 0.)),
                    xy(along(end, back, 0., -6.))
                )
            }
        ));
        s.push(number(
            if down {
                pt(end.x + 8., end.y - 9.)
            } else {
                pt(end.x - 6., end.y - 8.)
            },
            if r.one { "1" } else { "N" },
            if down { "start" } else { "end" },
        ));
        s.push("</g>".into());
    }
    s.push("</g>".into());
    s.push("<g class=\"rz-tables\">".into());
    for (i, v) in vs.iter().enumerate() {
        let b = &boxes[i];
        s.push(format!(
            "<g class=\"rz-t\" data-t=\"{}\" transform=\"translate({},{})\">{}</g>",
            esc(&v.table.name),
            js(b.x),
            js(b.y),
            card(v, &l, &id, painted)
        ));
    }
    s.push("</g></svg>".into());
    Ok(SvgResult {
        svg: s.concat(),
        width: w_,
        height: h_,
        background: l.stage.as_ref().map(|st| st.base.to_string()),
        boxes,
    })
}

#[cfg(test)]
mod tests {
    use super::{SvgOptions, js, round, to_svg};
    use serde_json::Value;

    /// 처음 갈리는 자리와 그 앞뒤
    fn first_diff(a: &str, b: &str) -> Option<String> {
        let i = a
            .bytes()
            .zip(b.bytes())
            .position(|(x, y)| x != y)
            .or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))?;
        let lo = a.floor_char_boundary(i.saturating_sub(160));
        let ctx =
            |s: &str| s[lo.min(s.len())..s.ceil_char_boundary((i + 160).min(s.len()))].to_string();
        Some(format!("@{i}\n기대 …{}…\n결과 …{}…", ctx(a), ctx(b)))
    }

    #[test]
    fn draws_like_resin() {
        let cases: Value =
            serde_json::from_str(include_str!("../../tests/resin_svg.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert_eq!(cases.len(), 28);
        for c in cases {
            let model = crate::resin::compile(c["source"].as_str().unwrap())
                .model
                .expect("예제는 오류가 없다");
            let options: SvgOptions = serde_json::from_value(c["options"].clone()).unwrap();
            let r = to_svg(&model, &options).unwrap();
            if let Some(d) = first_diff(c["svg"].as_str().unwrap(), &r.svg) {
                panic!("{}: {d}", c["name"]);
            }
            assert_eq!(
                (r.width, r.height),
                (c["width"].as_f64().unwrap(), c["height"].as_f64().unwrap())
            );
            assert_eq!(
                serde_json::to_value(&r.background).unwrap(),
                c["background"]
            );
            let want: Vec<(String, f64, f64, f64, f64)> = c["boxes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    let n = |k: &str| b[k].as_f64().unwrap();
                    (
                        b["table"].as_str().unwrap().to_string(),
                        n("x"),
                        n("y"),
                        n("w"),
                        n("h"),
                    )
                })
                .collect();
            let got: Vec<(String, f64, f64, f64, f64)> = r
                .boxes
                .iter()
                .map(|b| (b.table.clone(), b.x, b.y, b.w, b.h))
                .collect();
            assert_eq!(got, want, "{}", c["name"]);
        }
    }

    /// 실데이터(로컬 전용) — AMBER_REAL_SVG 에 resin-fixture.mjs 의 실데이터 출력을 주면 그 스키마들로도 견준다
    #[test]
    fn draws_real_schemas_like_resin() {
        let Ok(path) = std::env::var("AMBER_REAL_SVG") else {
            return;
        };
        let cases: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let cases = cases.as_array().unwrap();
        for (i, c) in cases.iter().enumerate() {
            let model = crate::resin::compile(c["source"].as_str().unwrap())
                .model
                .expect("생성한 원문은 오류가 없다");
            let options: SvgOptions = serde_json::from_value(c["options"].clone()).unwrap();
            let r = to_svg(&model, &options).unwrap();
            if let Some(d) = first_diff(c["svg"].as_str().unwrap(), &r.svg) {
                panic!("실데이터 {i}: {d}");
            }
        }
        eprintln!("실데이터 {} 장 같다", cases.len());
    }

    #[test]
    fn numbers_print_like_js() {
        assert_eq!(js(-0.), "0");
        assert_eq!(js(62.), "62");
        assert_eq!(js(0.035), "0.035");
        assert_eq!(js(1e21), "1e+21");
        assert_eq!(js(1e-7), "1e-7");
        assert_eq!(js(-12.5), "-12.5");
        assert_eq!(round(-2.5), -2.);
        assert_eq!(round(2.5), 3.);
        assert_eq!(round(0.49999999999999994), 0.);
    }
}
