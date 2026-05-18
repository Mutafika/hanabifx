//! hanabifx — press keys / clicks, get fireworks.
//!
//! 全画面の click-through 透明 NSWindow を 1 枚張って、 sabitori で
//! 2D パーティクルを描く単機能アプリ。 入力は addGlobalMonitorForEventsMatchingMask
//! で他アプリ宛イベントもフックして burst を spawn する。
//!
//! `--edit` フラグで起動すると、 primary view を設定パネルに、 fx 全画面を
//! sabitori の extra window に切り替える。 sabitori v1 の extra は input event
//! を受け取らないので、 GUI 操作の都合で「クリックを取りたい方」 を primary
//! にする必要があるため。
//!
//! v0 スコープ:
//! - 単一ディスプレイ (primary)。 multi-display は v0.1+
//! - 2D particle のみ。 3D (murakumo マテリアル) は v0.1+
//! - 設定 GUI: +/- ボタン (drag slider 無し) で各 param を編集、 変更は
//!   即時 fx 反映 + TOML 永続化

mod config;
mod fx_input_tap;
mod fx_particle;
mod settings_ui;

use std::sync::{Arc, Mutex};

use sabitori::{div, text, BackdropBlur, DeclarativeApp, Element, ExtraWindow, Px, ViewContext};
use settings_ui::Trigger;

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
    cfg: Arc<Mutex<config::Config>>,
    screen_size: (f32, f32),
    edit_mode: bool,
    active_trigger: Trigger,
    /// Primary window への参照。 set_window コールバックで埋まる。
    /// `Done` ボタンで primary を 1x1 click-through 化する時に必要。
    primary_window: Option<Arc<winit::window::Window>>,
    /// `--edit` 起動後に「Done」 を押された後の状態。 view() を空 div に
    /// 切り替えて余計な layout コストも避ける。
    settings_hidden: bool,
    /// 前フレームで粒子が居たかどうか。 粒子が「居る → 全消滅」 の遷移を
    /// 検知して、 消滅した最終フレームを 1 回余分に redraw するために使う。
    /// これが無いと poll_dirty が一足早く false を返してしまい、 直前
    /// フレームに描いた粒子が画面に焼き付いたまま残る (mouse 単発クリックで
    /// 顕在化、 連打する keyboard では新粒子が来るので気付きにくい)。
    had_particles_last_frame: bool,
}

impl HanabiFx {
    /// 設定変更を TOML に永続化。 失敗してもログだけ吐いて続行 (起動後に
    /// 書けない理由は通常 permission / disk full 系で、 fatal にしない方が
    /// アプリとして親切)。
    /// 設定パネル (primary window) を 1x1 click-through window 化して
    /// 「画面の隅に超小さく見えない状態」 で alive にする。
    ///
    /// 試行錯誤の経緯:
    /// - `[NSWindow orderOut:]` → 一瞬のフリーズ (macOS の hide 遷移)
    /// - winit `set_visible(false)` → mouse fx が止まる (loop pump 異常)
    /// - 画面外 `(−10000, −10000)` 移動 → particle tick 自体が止まる
    ///   (off-screen window は AppKit の active set から外れて run loop が
    ///   throttle される模様)
    ///
    /// 1x1 click-through を (0, 0) に置くだけなら winit/AppKit はそれを
    /// 正規の active window として扱い続けるので fx extra の redraw cadence
    /// が維持される。 view 側で `settings_hidden` フラグを見て空 div を返す
    /// ようにしてるので 1px の中身は空 = 完全に視認不能。
    #[cfg(target_os = "macos")]
    fn hide_settings_window(&mut self) {
        use winit::dpi::{LogicalPosition, LogicalSize};
        if let Some(window) = self.primary_window.as_ref() {
            let _ = window.request_inner_size(LogicalSize::new(1.0, 1.0));
            window.set_outer_position(LogicalPosition::new(0.0, 0.0));
            // click-through + 透明 (元の primary 透明設定を維持) + level 3 に
            // することで「もし 1px が見えても」 黒点が出ないようにする。
            configure_fx_window(window);
        }
        self.settings_hidden = true;
    }

    #[cfg(not(target_os = "macos"))]
    fn hide_settings_window(&mut self) {
        self.settings_hidden = true;
    }

    #[cfg(not(target_os = "macos"))]
    fn hide_settings_window(&self) {}

    fn persist_config(&self) {
        let snapshot = match self.cfg.lock() {
            Ok(g) => *g,
            Err(_) => return,
        };
        if let Err(e) = save_config(&snapshot) {
            eprintln!("[hanabifx] config persist failed: {e}");
        }
    }

    fn fx_overlay_view(&self) -> Element {
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
}

impl DeclarativeApp for HanabiFx {
    fn title(&self) -> &str {
        "hanabifx"
    }

    fn size(&self) -> (f32, f32) {
        if self.edit_mode {
            settings_ui::panel_size()
        } else {
            self.screen_size
        }
    }

    fn position(&self) -> Option<(f32, f32)> {
        if self.edit_mode {
            // 画面中央に置く。 fx の live preview は cursor 周りに散るので、
            // パネル自体に隠れる範囲は妥協してもらう前提。
            let (pw, ph) = settings_ui::panel_size();
            Some((
                (self.screen_size.0 - pw) * 0.5,
                (self.screen_size.1 - ph) * 0.5,
            ))
        } else {
            Some((0.0, 0.0))
        }
    }

    fn min_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }

    fn decorations(&self) -> bool {
        false
    }

    fn transparent(&self) -> bool {
        // 常に透明。 edit モードの settings パネル本体は自前で .bg(BG) を
        // 95% alpha で塗ってるので透明 NSWindow でも見た目は崩れない。
        // 透明にしておくと Done 後に primary を 1x1 click-through 化して
        // 「実質非表示・event loop は alive」 状態にできる (winit/AppKit に
        // window 不在を悟られないので fx tick が止まらない)。
        true
    }

    fn backdrop_blur(&self) -> Option<BackdropBlur> {
        None
    }

    /// Primary window の NSWindow 設定。 edit モード時は通常 panel、 そうで
    /// なければ fx click-through オーバーレイ。
    #[cfg(target_os = "macos")]
    fn macos_configure_window(&self, window: &winit::window::Window) {
        if self.edit_mode {
            configure_settings_window(window);
        } else {
            configure_fx_window(window);
        }
    }

    fn extra_windows(&self) -> Vec<ExtraWindow> {
        if self.edit_mode {
            vec![ExtraWindow {
                key: "fx".to_string(),
                title: "hanabifx-fx".to_string(),
                size: self.screen_size,
                position: Some((0.0, 0.0)),
                min_size: (1.0, 1.0),
                transparent: true,
                decorations: false,
                backdrop_blur: None,
                backdrop_blur_top_strip_height: None,
                scene_3d: false,
            }]
        } else {
            vec![]
        }
    }

    #[cfg(target_os = "macos")]
    fn macos_configure_extra_window(&self, key: &str, window: &winit::window::Window) {
        if key == "fx" {
            configure_fx_window(window);
        }
    }

    fn view_for(&self, key: &str, _ctx: &ViewContext) -> Element {
        if key == "fx" {
            self.fx_overlay_view()
        } else {
            div()
        }
    }

    fn view(&self, _ctx: &ViewContext) -> Element {
        if self.edit_mode {
            if self.settings_hidden {
                // Done 後: primary は 1x1 click-through window として残ってる
                // (event loop 駆動のためだけに alive)。 view 出力は空でいい。
                return div();
            }
            let cfg = match self.cfg.lock() {
                Ok(g) => *g,
                Err(_) => config::Config::default(),
            };
            settings_ui::view(&cfg, self.active_trigger)
        } else {
            self.fx_overlay_view()
        }
    }

    fn on_click(&mut self, id: &str) {
        if !self.edit_mode {
            return;
        }
        let mut cfg_guard = match self.cfg.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match settings_ui::handle_click(id, &mut cfg_guard, self.active_trigger) {
            settings_ui::ClickEffect::None => {}
            settings_ui::ClickEffect::Changed => {
                drop(cfg_guard);
                self.persist_config();
            }
            settings_ui::ClickEffect::SwitchTab(t) => {
                drop(cfg_guard);
                self.active_trigger = t;
            }
            settings_ui::ClickEffect::HideSettings => {
                drop(cfg_guard);
                self.hide_settings_window();
            }
            settings_ui::ClickEffect::Quit => {
                std::process::exit(0);
            }
        }
    }

    fn set_window(&mut self, window: Arc<winit::window::Window>) {
        self.primary_window = Some(window);
    }

    fn lazy_render(&self) -> bool {
        true
    }

    fn poll_dirty(&mut self) -> bool {
        let alive = self
            .fx_state
            .lock()
            .map(|s| !s.particles.is_empty())
            .unwrap_or(false);
        if alive {
            self.had_particles_last_frame = true;
            return true;
        }
        // 直前フレームに粒子が居て、 今フレームで全部 retain で消えた。
        // この 1 回だけ true を返して画面をクリアさせる。 これを怠ると
        // 直前フレームに描いた粒子が wgpu surface に焼き付いたまま残る。
        if self.had_particles_last_frame {
            self.had_particles_last_frame = false;
            return true;
        }
        false
    }

    fn target_frame_interval(&self) -> std::time::Duration {
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

/// fx オーバーレイ NSWindow: click-through, level 3 floating, all-spaces, 透明。
#[cfg(target_os = "macos")]
fn configure_fx_window(window: &winit::window::Window) {
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
        let _: () = msg_send![ns_window, setIgnoresMouseEvents: Bool::YES];
        let _: () = msg_send![ns_window, setLevel: 3_i64];
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

/// 設定パネル NSWindow: 通常の floating panel、 mouse 受け取る、 dark appearance。
#[cfg(target_os = "macos")]
fn configure_settings_window(window: &winit::window::Window) {
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
        // floating window level (3) より上、 menu bar (25) より下に置く。
        // ユーザーがメニューバーから操作したい時のために 25 ではなく 4 にする。
        let _: () = msg_send![ns_window, setLevel: 4_i64];
        // canJoinAllSpaces | stationary。 Mission Control に出てしまわない
        // ように ignoresCycle も加える。
        let cb: u64 = (1 << 0) | (1 << 4) | (1 << 6);
        let _: () = msg_send![ns_window, setCollectionBehavior: cb];
        let _: () = msg_send![ns_window, setAcceptsMouseMovedEvents: Bool::YES];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: Bool::NO];
        // ダーク見た目。
        if let Some(appearance_cls) = AnyClass::get("NSAppearance") {
            use objc2_foundation::NSString;
            let name = NSString::from_str("NSAppearanceNameDarkAqua");
            let appearance: *mut AnyObject = msg_send![
                appearance_cls,
                appearanceNamed: &*name
            ];
            if !appearance.is_null() {
                let _: () = msg_send![ns_window, setAppearance: appearance];
            }
        }
    }
}

/// `Config::config_path` と同じ場所に書き戻す。 載っているフィールドだけ
/// pretty toml で出力。
fn save_config(cfg: &config::Config) -> std::io::Result<()> {
    let path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("hanabifx")
        .join("config.toml");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, serialized)
}

fn main() {
    let edit_mode = std::env::args().any(|a| a == "--edit");

    // macOS の activation policy。 edit モードでは Regular にして dock icon を
    // 出し、 アプリ切替対象にする (キー入力受け付けに必要)。 fx-only モードでは
    // Accessory にして dock icon を隠す。
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            let policy = if edit_mode {
                NSApplicationActivationPolicy::Regular
            } else {
                NSApplicationActivationPolicy::Accessory
            };
            app.setActivationPolicy(policy);
        }
    }

    let cfg = Arc::new(Mutex::new(config::Config::load_or_create()));
    let fx_state = Arc::new(Mutex::new(fx_particle::FxState::default()));
    let screen_size = primary_screen_size();

    // 入力タップ。
    let tap_state = fx_state.clone();
    let tap_cfg = cfg.clone();
    let screen_h = screen_size.1;
    fx_input_tap::start(screen_h, move |trigger, x, y, typed_label| {
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

    let app = HanabiFx {
        fx_state,
        cfg,
        screen_size,
        edit_mode,
        active_trigger: Trigger::Mouse,
        primary_window: None,
        settings_hidden: false,
        had_particles_last_frame: false,
    };
    sabitori::run_declarative(app);
}
