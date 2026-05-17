//! hanabifx — press keys / clicks, get fireworks.
//!
//! 全画面の click-through 透明 NSWindow を 1 枚張って、 sabitori で
//! 2D パーティクルを描く単機能アプリ。 入力は addGlobalMonitorForEventsMatchingMask
//! で他アプリ宛イベントもフックして burst を spawn する。
//!
//! v0 スコープ:
//! - 単一ディスプレイ (primary)。 multi-display は v0.1+
//! - 2D particle のみ。 3D (murakumo マテリアル) は v0.1+
//! - 設定は `~/.config/hanabifx/config.toml` 読み込み。 live 反映なし
//!
//! 既存実装である matcha-shell の fx 機能から独立切り出し。 matcha は
//! menu bar / dock / launcher 等と event loop を共有していたため fx の
//! cadence と他機能の cadence がコンフリクトしていた。 hanabifx は
//! event loop も renderer も fx 専用なので 120Hz / vsync 整列が自由。

mod config;
mod fx_input_tap;
mod fx_particle;

use std::sync::{Arc, Mutex};

use sabitori::{div, text, BackdropBlur, DeclarativeApp, Element, Px, ViewContext};

#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::{AnyClass, AnyObject, Bool}};
#[cfg(target_os = "macos")]
use objc2_app_kit::NSScreen;
#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// プライマリ NSScreen の論理サイズ (top-left origin)。 NSScreen が
/// 存在しないヘッドレス環境では 1920x1080 にフォールバック。
#[cfg(target_os = "macos")]
fn primary_screen_size() -> (f32, f32) {
    let mtm = match MainThreadMarker::new() {
        Some(m) => m,
        None => return (1920.0, 1080.0),
    };
    unsafe {
        let screens = NSScreen::screens(mtm);
        if screens.count() == 0 {
            return (1920.0, 1080.0);
        }
        let primary = screens.objectAtIndex(0);
        let frame = primary.frame();
        (frame.size.width as f32, frame.size.height as f32)
    }
}

#[cfg(not(target_os = "macos"))]
fn primary_screen_size() -> (f32, f32) {
    (1920.0, 1080.0)
}

struct HanabiFx {
    fx_state: Arc<Mutex<fx_particle::FxState>>,
    screen_size: (f32, f32),
}

impl DeclarativeApp for HanabiFx {
    fn title(&self) -> &str {
        "hanabifx"
    }

    fn size(&self) -> (f32, f32) {
        self.screen_size
    }

    fn position(&self) -> Option<(f32, f32)> {
        Some((0.0, 0.0))
    }

    fn min_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }

    fn decorations(&self) -> bool {
        false
    }

    fn transparent(&self) -> bool {
        true
    }

    fn backdrop_blur(&self) -> Option<BackdropBlur> {
        None
    }

    /// Click-through, level 3 floating, visible on every Space。 matcha-shell
    /// の fx 用 extra と同じ NSWindow セットアップを直接 primary に当てる。
    #[cfg(target_os = "macos")]
    fn macos_configure_window(&self, window: &winit::window::Window) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let appkit = match handle.as_raw() {
            RawWindowHandle::AppKit(h) => h,
            _ => return,
        };
        let ns_view: *mut AnyObject = appkit.ns_view.as_ptr() as *mut AnyObject;
        unsafe {
            let ns_window: *mut AnyObject = msg_send![ns_view, window];
            if ns_window.is_null() {
                return;
            }
            // 全面 click-through。 fx は装飾用、 マウスイベントは絶対に
            // 受け取らない。
            let _: () = msg_send![ns_window, setIgnoresMouseEvents: Bool::YES];
            // NSFloatingWindowLevel = 3。 通常アプリ (0) より上、 menu bar
            // (NSStatusWindowLevel = 25) より下。
            let _: () = msg_send![ns_window, setLevel: 3_i64];
            // canJoinAllSpaces | stationary | ignoresCycle。
            // 全 Space で見える + Mission Control / Cmd-Tab に出ない。
            let cb: u64 = (1 << 0) | (1 << 4) | (1 << 6);
            let _: () = msg_send![ns_window, setCollectionBehavior: cb];
            let _: () = msg_send![ns_window, setHasShadow: Bool::NO];
            let _: () = msg_send![ns_window, setMovable: Bool::NO];
            let _: () = msg_send![ns_window, setMovableByWindowBackground: Bool::NO];
            let _: () = msg_send![ns_window, setOpaque: Bool::NO];
            let color_cls = AnyClass::get("NSColor").expect("NSColor");
            let clear: *mut AnyObject = msg_send![color_cls, clearColor];
            let _: () = msg_send![ns_window, setBackgroundColor: clear];
        }
    }

    fn view(&self, _ctx: &ViewContext) -> Element {
        // 全画面 root。 view() は sabitori の毎フレーム build path から
        // 呼ばれるので、 ここで particles を Element 木に変換する。
        let st = match self.fx_state.lock() {
            Ok(g) => g,
            Err(_) => return div(),
        };
        let particles: Vec<Element> = st
            .particles
            .iter()
            .map(|p| {
                use fx_particle::ShapeKind;
                let d = p.radius * 2.0;
                let alpha = p.alpha();
                let c = p.color.with_alpha(alpha);
                match p.shape {
                    ShapeKind::Disc => div()
                        .pos(p.x - p.radius, p.y - p.radius)
                        .w(Px(d))
                        .h(Px(d))
                        .rounded_px(p.radius)
                        .bg(c),
                    ShapeKind::Square => div()
                        .pos(p.x - p.radius, p.y - p.radius)
                        .w(Px(d))
                        .h(Px(d))
                        .rounded_px(1.5)
                        .bg(c),
                    ShapeKind::Ring => div()
                        .pos(p.x - p.radius, p.y - p.radius)
                        .w(Px(d))
                        .h(Px(d))
                        .rounded_px(p.radius)
                        .border(2.0, c),
                    ShapeKind::Glyph(ch) => {
                        let font_size = p.radius * 2.6;
                        div()
                            .pos(p.x - p.radius * 1.2, p.y - p.radius * 1.2)
                            .w(Px(p.radius * 2.4))
                            .h(Px(p.radius * 2.4))
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .children([text(ch.to_string()).font_size(font_size).color(c)])
                    }
                }
            })
            .collect();
        div()
            .w(Px(self.screen_size.0))
            .h(Px(self.screen_size.1))
            .children(particles)
    }

    fn lazy_render(&self) -> bool {
        true
    }

    fn poll_dirty(&mut self) -> bool {
        self.fx_state
            .lock()
            .map(|s| !s.particles.is_empty())
            .unwrap_or(false)
    }

    fn target_frame_interval(&self) -> std::time::Duration {
        // fx 動作中だけ 120Hz、 完全 idle 時は 60Hz。 sabitori の default は
        // 8ms だが、 idle 時の CPU を更に節約するため override する。
        let alive = self
            .fx_state
            .lock()
            .map(|s| !s.particles.is_empty())
            .unwrap_or(false);
        if alive {
            std::time::Duration::from_millis(8)
        } else {
            std::time::Duration::from_millis(16)
        }
    }

    fn tick(&mut self, dt: f32) {
        if let Ok(mut s) = self.fx_state.lock() {
            s.tick(dt);
        }
    }
}

fn main() {
    // macOS の activation policy を accessory に設定して dock icon と
    // menu bar アプリ名を出さない。 hanabifx は背景常駐型なので。
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    }

    let cfg = Arc::new(Mutex::new(config::Config::load_or_create()));
    let fx_state = Arc::new(Mutex::new(fx_particle::FxState::default()));
    let screen_size = primary_screen_size();

    // 入力タップ。 fx_input_tap::start は global + local monitor を両方張る。
    // local は --edit 等で hanabifx 自身がフォーカスを持つ場合の保険だが、
    // hanabifx は常時 click-through なので実質 global しか走らない。 害は無い。
    let tap_state = fx_state.clone();
    let tap_cfg = cfg.clone();
    let screen_h = screen_size.1;
    fx_input_tap::start(screen_h, move |trigger, x, y, typed_label| {
        // Mutex 復旧 — poisoned でも先のセッションで破損していなければ続行。
        tap_cfg.clear_poison();
        tap_state.clear_poison();
        let snapshot = match tap_cfg.lock() {
            Ok(g) => *g,
            Err(_) => return,
        };
        if !snapshot.enabled {
            return;
        }
        let params = match trigger {
            fx_input_tap::FxTrigger::Mouse => snapshot.mouse,
            fx_input_tap::FxTrigger::MouseRight => snapshot.right,
            fx_input_tap::FxTrigger::MouseDouble => snapshot.double,
            fx_input_tap::FxTrigger::Key => snapshot.key,
        };
        if let Ok(mut s) = tap_state.lock() {
            s.burst(x, y, &params, typed_label.as_deref());
        }
    });

    drop(cfg); // 設定は input tap clone 側 (tap_cfg) が保持する。 app 構造体は不要。
    let app = HanabiFx {
        fx_state,
        screen_size,
    };
    sabitori::run_declarative(app);
}
