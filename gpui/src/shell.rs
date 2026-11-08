// 앱 껍데기 — 지금 앱 src-tauri/lib.rs 의 창, 종료 정리.
//  - 데이터 폴더: ~/Library/Application Support/dev.jhzlo.amber (지금 앱과 같은 DB, vault)
//  - 창 자리: tauri-plugin-window-state 와 같은 파일, 같은 꼴(.window-state.json, 물리 픽셀) — 두 앱을 번갈아 써도 이어진다
//  - 종료할 때 AI CLI 자식 프로세스를 정리한다

use std::path::{Path, PathBuf};

use gpui_kit::*;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::Spike;

const APP_ID: &str = "dev.jhzlo.amber";
const STATE_FILE: &str = ".window-state.json";
pub(crate) const MAIN: &str = "main";
pub(crate) const WIDGET: &str = "widget";

/// 앱 데이터 폴더의 amber.db — 없으면 폴더부터 만든다(처음 실행)
pub(crate) fn db_path() -> String {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let dir = home.join("Library/Application Support").join(APP_ID);
    let _ = std::fs::create_dir_all(&dir);
    dir.join("amber.db").to_string_lossy().into_owned()
}

/// 창 하나의 자리 — 위치는 창 틀의 왼쪽 위, 크기는 내용 크기. 최대화 중에는 크기, 위치를 갱신하지 않고
/// 되살릴 때 prev_x/prev_y 에 둔다(플러그인과 같다)
#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq)]
struct Saved {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    prev_x: i32,
    prev_y: i32,
    maximized: bool,
    visible: bool,
    decorated: bool,
    fullscreen: bool,
}

fn read_all(dir: &Path) -> serde_json::Map<String, Json> {
    std::fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn load(dir: &Path, label: &str) -> Option<Saved> {
    serde_json::from_value::<Saved>(read_all(dir).get(label)?.clone())
        .ok()
        .filter(|s| *s != Saved::default())
}

/// 지금 자리를 적는다(visible = 지금 보이는가)
pub(crate) fn save_window(dir: &Path, label: &str, window: &Window, visible: bool) {
    let mut all = read_all(dir);
    let mut s = all
        .get(label)
        .and_then(|v| serde_json::from_value::<Saved>(v.clone()).ok())
        .unwrap_or_default();
    let scale = window.scale_factor();
    let phys = |v: Pixels| (v.as_f32() * scale).round() as i32;
    s.maximized = window.is_maximized();
    s.fullscreen = window.is_fullscreen();
    s.visible = visible;
    s.decorated = label == MAIN;
    if !s.maximized && !s.fullscreen {
        let frame = window.bounds();
        let content = window.viewport_size();
        s.width = phys(content.width).max(1) as u32;
        s.height = phys(content.height).max(1) as u32;
        s.x = phys(frame.origin.x);
        s.y = phys(frame.origin.y);
        s.prev_x = s.x;
        s.prev_y = s.y;
    }
    if let Ok(v) = serde_json::to_value(s) {
        all.insert(label.into(), v);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&all) {
        let _ = std::fs::write(dir.join(STATE_FILE), bytes);
    }
}

/// 적어 둔 자리를 화면 좌표로 — 어느 화면에도 걸치지 않으면 None(OS 가 놓게 한다)
fn restore_bounds(s: &Saved, size_px: Size<Pixels>, cx: &App) -> Option<Bounds<Pixels>> {
    let scale = screen_scale();
    let at = |v: i32| px(v as f32 / scale);
    let origin = if s.maximized {
        point(at(s.prev_x), at(s.prev_y))
    } else {
        point(at(s.x), at(s.y))
    };
    let b = Bounds::new(origin, size_px);
    cx.displays()
        .iter()
        .any(|d| d.bounds().intersects(&b))
        .then_some(b)
}

/// 메인 창 자리 — 처음이면 지금 앱의 기본 크기(1000x720) 가운데
pub(crate) fn main_bounds(dir: &Path, cx: &App) -> WindowBounds {
    let fallback = |sz| Bounds::centered(None, sz, cx);
    let Some(s) = load(dir, MAIN) else {
        return WindowBounds::Windowed(fallback(size(px(1000.), px(720.))));
    };
    let scale = screen_scale();
    let sz = size(px(s.width as f32 / scale), px(s.height as f32 / scale));
    let b = restore_bounds(&s, sz, cx).unwrap_or_else(|| fallback(sz));
    if s.fullscreen {
        WindowBounds::Fullscreen(b)
    } else if s.maximized {
        WindowBounds::Maximized(b)
    } else {
        WindowBounds::Windowed(b)
    }
}

/// 위젯 자리 — 크기는 늘 같다(늘이지 못하는 창)
pub(crate) fn widget_bounds(dir: &Path, size_px: Size<Pixels>, cx: &App) -> Option<Bounds<Pixels>> {
    restore_bounds(&load(dir, WIDGET)?, size_px, cx)
}

/// 주 화면의 배율 — 창을 열기 전이라 창에 물을 수 없다
fn screen_scale() -> f32 {
    let Some(cls) = AnyClass::get(c"NSScreen") else {
        return 2.;
    };
    let screen: *mut AnyObject = unsafe { msg_send![cls, mainScreen] };
    if screen.is_null() {
        return 2.;
    }
    let s: f64 = unsafe { msg_send![screen, backingScaleFactor] };
    s as f32
}

/// 위젯이 떠 있나 — 숨긴 위젯은 창이 닫혀 손잡이가 낡는다
fn widget_open(spike: &Entity<Spike>, cx: &mut App) -> Option<WindowHandle<crate::widget::Widget>> {
    let h = spike.read(cx).widget?;
    h.update(cx, |_, _, _| ()).ok().map(|_| h)
}

/// 실제 앱에만(찍기, 시험대는 부르지 않는다) — 메인 창이 뜬 뒤
pub(crate) fn install(main_window: AnyWindowHandle, spike: Entity<Spike>, cx: &mut App) {
    let dir = spike.read(cx).db.data_dir();
    // 셸 PATH 를 미리 읽어 둔다 — Dock 으로 뜬 앱도 터미널과 같은 곳에서 CLI 를 찾고, 첫 AI 호출이 셸 기동을 기다리지 않게
    drop(crate::ai::spawn(amber_core::shellenv::path()));

    let weak = spike.downgrade();
    cx.on_app_quit(move |cx| {
        let _ = main_window.update(cx, |_, window, _| save_window(&dir, MAIN, window, true));
        if let Some(spike) = weak.upgrade()
            && let Some(h) = widget_open(&spike, cx)
        {
            let _ = h.update(cx, |_, window, _| save_window(&dir, WIDGET, window, true));
        }
        // AI CLI 와 그것이 띄운 MCP 서버가 앱보다 오래 살지 않게
        amber_core::ai::kill_all();
        async {}
    })
    .detach();

    // 위젯을 띄워 둔 채 끝냈으면 다시 띄운다(플러그인의 visible)
    if load(&spike.read(cx).db.data_dir(), WIDGET).is_some_and(|s| s.visible) {
        let _ = main_window.update(cx, |_, window, cx| {
            spike.update(cx, |s, cx| s.open_widget(window, cx));
        });
    }
}
