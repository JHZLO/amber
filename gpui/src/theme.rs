// 색 토큰 — 지금 앱 styles.css 의 :root(라이트), :root[data-theme="dark"](다크) 값 그대로(DESIGN.md §2).
// 화면은 토큰 이름으로만 색을 쓴다. gpui-component(입력칸, 체크박스, 마크다운)도 같은 토큰을 보게 맞춘다.
// 테마는 지금 앱의 기본값(system)처럼 macOS 화면 모드를 따른다.

use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{App, BoxShadow, Global, Hsla, Rgba, WindowAppearance, point, px, rgb};

#[derive(Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub surface_3: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    pub text_3: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub primary_bg: Hsla,
    pub primary_fg: Hsla,
    pub primary_bg_hover: Hsla,
    pub danger: Hsla,
    pub danger_soft: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    /// 달력의 일요일(공휴일)/토요일, 휴가(호박색 — 결과 색 예외 둘 중 하나)
    pub cal_sun: Hsla,
    pub cal_sat: Hsla,
    pub vacation: Hsla,
    /// --glass-raise(면 78%): 눌린 탭처럼 판에서 한 단 솟은 유리의 바탕
    pub glass_raise: Hsla,
    /// --shadow-sm
    pub shadow_sm: Hsla,
    /// 유리(§2 유리): 손이 닿은 면 / 강조(활성). CSS 는 사선 그라디언트 세 칸인데 여기선 양 끝 두 칸으로 근사한다
    pub glass_fill: (Hsla, Hsla),
    pub glass_edge: Hsla,
    /// 손이 닿은 유리(--glass-fill-hover, --glass-edge-hover)
    pub glass_fill_hover: (Hsla, Hsla),
    pub glass_edge_hover: Hsla,
    pub glass_fill_accent: (Hsla, Hsla),
    pub glass_fill_accent_hover: (Hsla, Hsla),
    pub glass_edge_accent: Hsla,
    /// --glass-sheen(위쪽 한 줄기 빛, 늘 흰색) / 손이 닿았을 때
    pub sheen: Hsla,
    pub sheen_hover: Hsla,
    /// --glass-seat: 안쪽 아래 한 줄(유리 두께) + 바깥 그림자 0 1px 2px
    pub seat_in: Hsla,
    pub seat: Hsla,
    /// --glass-seat-hover 의 바깥 그림자(0 2px 6px) — 손이 닿으면 한 단 들린다
    pub seat_hover: Hsla,
}

impl Global for Tokens {}

fn c(v: u32) -> Hsla {
    rgb(v).into()
}

fn a(r: u8, g: u8, b: u8, alpha: f32) -> Hsla {
    Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: alpha,
    }
    .into()
}

const LIGHT: fn() -> Tokens = || Tokens {
    dark: false,
    surface: c(0xffffff),
    surface_2: c(0xf4f4f5),
    surface_3: c(0xe9e9ec),
    text: c(0x18181b),
    text_2: c(0x52525b),
    text_3: c(0x71717a),
    border: c(0xececee),
    border_strong: c(0xdcdce0),
    primary_bg: c(0x18181b),
    primary_fg: c(0xffffff),
    primary_bg_hover: c(0x333333),
    danger: c(0xdc2626),
    danger_soft: c(0xfef2f2),
    success: c(0x1a7f37),
    warning: c(0xa16207),
    cal_sun: c(0xb91c1c),
    cal_sat: c(0x1d4ed8),
    vacation: c(0xa16207),
    glass_raise: a(255, 255, 255, 0.78),
    shadow_sm: a(24, 24, 27, 0.06),
    glass_fill: (a(24, 24, 27, 0.05), a(24, 24, 27, 0.04)),
    glass_edge: a(24, 24, 27, 0.12),
    glass_fill_hover: (a(24, 24, 27, 0.1), a(24, 24, 27, 0.075)),
    glass_edge_hover: a(24, 24, 27, 0.2),
    glass_fill_accent: (a(24, 24, 27, 0.16), a(24, 24, 27, 0.125)),
    glass_fill_accent_hover: (a(24, 24, 27, 0.24), a(24, 24, 27, 0.185)),
    glass_edge_accent: a(24, 24, 27, 0.34),
    sheen: a(255, 255, 255, 0.6),
    sheen_hover: a(255, 255, 255, 0.8),
    seat_in: a(24, 24, 27, 0.08),
    seat: a(24, 24, 27, 0.1),
    seat_hover: a(24, 24, 27, 0.14),
};

const DARK: fn() -> Tokens = || Tokens {
    dark: true,
    surface: c(0x161618),
    surface_2: c(0x1f1f23),
    surface_3: c(0x2a2a2f),
    text: c(0xf4f4f5),
    text_2: c(0xa1a1aa),
    text_3: c(0x8b8b95),
    border: c(0x26262a),
    border_strong: c(0x3a3a40),
    primary_bg: c(0xf4f4f5),
    primary_fg: c(0x18181b),
    primary_bg_hover: c(0xe4e4e7),
    danger: c(0xf87171),
    danger_soft: c(0x2b1a1c),
    success: c(0x4ade80),
    warning: c(0xfcd34d),
    cal_sun: c(0xfca5a5),
    cal_sat: c(0x93b4ff),
    vacation: c(0xfcd34d),
    glass_raise: a(22, 22, 24, 0.78),
    shadow_sm: a(0, 0, 0, 0.4),
    glass_fill: (a(255, 255, 255, 0.05), a(255, 255, 255, 0.04)),
    glass_edge: a(255, 255, 255, 0.09),
    glass_fill_hover: (a(255, 255, 255, 0.1), a(255, 255, 255, 0.075)),
    glass_edge_hover: a(255, 255, 255, 0.18),
    glass_fill_accent: (a(255, 255, 255, 0.18), a(255, 255, 255, 0.14)),
    glass_fill_accent_hover: (a(255, 255, 255, 0.26), a(255, 255, 255, 0.2)),
    glass_edge_accent: a(255, 255, 255, 0.3),
    sheen: a(255, 255, 255, 0.1),
    sheen_hover: a(255, 255, 255, 0.24),
    seat_in: a(0, 0, 0, 0.35),
    seat: a(0, 0, 0, 0.5),
    seat_hover: a(0, 0, 0, 0.55),
};

impl Tokens {
    pub fn get(cx: &App) -> Tokens {
        *cx.global::<Tokens>()
    }

    /// 글자색을 alpha 만큼 — CSS 의 color-mix(in srgb, var(--text) N%, transparent)
    pub fn text_mix(&self, alpha: f32) -> Hsla {
        self.text.opacity(alpha)
    }

    /// 바깥 테 — CSS 의 0 0 0 Npx color
    pub fn outer_ring(color: Hsla, width: f32) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(width),
            inset: false,
        }
    }

    /// 안쪽 테 — CSS 의 inset 0 0 0 Npx color
    pub fn ring(color: Hsla, width: f32) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(width),
            inset: true,
        }
    }

    /// 손이 닿은 유리의 겹: 빛줄기(hover) + 두께 + 한 단 들린 그림자(--glass-seat-hover)
    pub fn glass_shadow_hover(&self) -> Vec<BoxShadow> {
        let mut v = self.glass_shadow(true);
        v[2] = BoxShadow {
            color: self.seat_hover,
            offset: point(px(0.), px(2.)),
            blur_radius: px(6.),
            spread_radius: px(0.),
            inset: false,
        };
        v
    }

    /// --shadow: 떠 있는 것(끌고 있는 줄, 메뉴)
    pub fn shadow(&self) -> Vec<BoxShadow> {
        let (far, near) = if self.dark {
            (a(0, 0, 0, 0.6), a(0, 0, 0, 0.45))
        } else {
            (a(24, 24, 27, 0.12), a(24, 24, 27, 0.07))
        };
        let layer = |color, y: f32, blur: f32, spread: f32| BoxShadow {
            color,
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        };
        vec![layer(far, 10., 30., -8.), layer(near, 4., 10., -4.)]
    }

    /// 유리의 그림자 겹: 빛줄기(sheen 또는 sheen-hover) + 두께(seat). CSS 의 box-shadow 와 같은 순서
    pub fn glass_shadow(&self, bright: bool) -> Vec<BoxShadow> {
        let line = |color, y: f32| BoxShadow {
            color,
            offset: point(px(0.), px(y)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        };
        vec![
            line(if bright { self.sheen_hover } else { self.sheen }, 0.5),
            line(self.seat_in, -0.5),
            BoxShadow {
                color: self.seat,
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }
}

/// 토큰을 바꾸고 gpui-component 테마에도 옮긴다. pref 는 지금 앱의 til.theme — "light", "dark", 없으면 macOS 화면 모드
pub fn apply(pref: Option<&str>, appearance: WindowAppearance, cx: &mut App) {
    let dark = match pref {
        Some("dark") => true,
        Some("light") => false,
        _ => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    let t = if dark { DARK() } else { LIGHT() };
    cx.set_global(t);
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        // --font 의 첫 두 칸(Pretendard)은 깔려 있지 않으면 건너뛴다 — 지금 앱이 실제로 그리는 글꼴이 이것이다
        theme.font_family = "Apple SD Gothic Neo".into();
        let k = &mut theme.colors;
        k.background = t.surface;
        k.foreground = t.text;
        k.border = t.border;
        k.input = t.border_strong;
        k.ring = t.text_3;
        k.muted = t.surface_2;
        k.muted_foreground = t.text_3;
        k.accent = t.surface_2;
        k.accent_foreground = t.text;
        k.secondary = t.surface_2;
        k.secondary_foreground = t.text;
        k.popover = t.surface;
        k.popover_foreground = t.text;
        k.primary = t.primary_bg;
        k.primary_foreground = t.primary_fg;
        k.primary_hover = t.primary_bg_hover;
        k.danger = t.danger;
        k.success = t.success;
        k.warning = t.warning;
        k.link = t.text;
        k.caret = t.text;
        k.selection = a(128, 128, 140, 0.28);
    });
}
