//! Global + Local NSEvent monitor for keyDown / leftMouseDown /
//! rightMouseDown plus a software double-click detector.
//!
//! `NSEvent.addGlobalMonitorForEventsMatchingMask:` fires for events
//! delivered to *other* apps, not to our own process — covers the
//! normal case where the fx overlay is click-through and inputs go
//! to whichever app the user is using. Inside the handler we discard
//! the event payload (no keylogging, only "something happened" +
//! event kind) and read the live cursor location from
//! `[NSEvent mouseLocation]`.
//!
//! `addLocalMonitorForEventsMatchingMask:` covers the *opposite*
//! case: events that come to matcha-shell itself. This matters when
//! `--edit` mode (Settings GUI) is open — matcha-shell sets
//! `setIgnoresMouseEvents:NO` on its panel-level NSWindow so the
//! modal can be interacted with, which means clicks land on
//! matcha-shell instead of going through to other apps. Without the
//! local monitor those clicks would silently skip the fx burst path
//! and the user would lose their live preview of slider tweaks. The
//! local handler returns the event unchanged (no swallowing) so
//! sabitori still routes the click to its own UI.
//!
//! Double-click is detected in software: two left-clicks within
//! `[NSEvent doubleClickInterval]` and within
//! `DOUBLE_CLICK_DISTANCE_PX` of each other promote the 2nd one to
//! an extra `MouseDouble` burst (the 1st `Mouse` burst still fires
//! immediately — see comment in `start` for the UX rationale).
//!
//! Caller passes the screen height (logical px, top-left-origin
//! coordinate system) so we can flip the bottom-origin macOS cursor Y.

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::Mutex;
    use std::time::Instant;

    use block2::RcBlock;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSPoint;

    const NSEVENT_MASK_LEFT_MOUSE_DOWN: u64 = 1 << 1;
    const NSEVENT_MASK_RIGHT_MOUSE_DOWN: u64 = 1 << 3;
    const NSEVENT_MASK_KEY_DOWN: u64 = 1 << 10;

    const NSEVENT_TYPE_LEFT_MOUSE_DOWN: u64 = 1;
    const NSEVENT_TYPE_RIGHT_MOUSE_DOWN: u64 = 3;
    const NSEVENT_TYPE_KEY_DOWN: u64 = 10;

    /// Max distance (logical pixels) between two clicks that still
    /// count as a "double click." macOS itself uses something around
    /// 5–8 px depending on input device; 8 is a comfortable middle.
    const DOUBLE_CLICK_DISTANCE_PX: f32 = 8.0;

    /// Which trigger fired the burst. Each is wired to its own
    /// `FxParams` in `FxCfg` so the user can give every input modality
    /// a distinct look.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum FxTrigger {
        /// Left mouse click. Also fires as the *first* of a
        /// double-click pair — the 2nd click additionally emits
        /// [`MouseDouble`].
        Mouse,
        /// Right mouse click. Independent from `Mouse`; the user can
        /// configure a completely different burst look.
        MouseRight,
        /// Two left-clicks in quick succession at nearly the same
        /// point. Emitted *in addition to* the 2nd `Mouse` so a
        /// double-click reads as "click-then-amplified" rather than
        /// "click [silence] thump."
        MouseDouble,
        /// Any key press (we never inspect which key — privacy).
        Key,
    }

    /// State for software double-click detection, shared between the
    /// `start` setup and each block invocation. `Mutex` because the
    /// global event monitor's block is `Fn` (block2 only accepts
    /// shared closures), but we still need to mutate the last-click
    /// timestamp / position.
    struct ClickState {
        last_left: Option<(Instant, f32, f32)>,
        /// `[NSEvent doubleClickInterval]` snapshot in seconds. Read
        /// once at startup; system pref changes mid-session won't
        /// retune, which matches every other matcha hotkey
        /// behavior.
        double_interval_s: f32,
    }

    /// Register a system-wide event observer. `callback` must be `Fn`
    /// (block2 only accepts immutable closures); use interior
    /// mutability inside if you need shared state.
    ///
    /// `screen_h` is the logical-pixel height of the main screen,
    /// used to flip macOS's bottom-origin mouse Y to top-left-origin.
    ///
    /// The `Option<String>` callback arg carries the printable label
    /// for the key that fired (used by the `Typed` particle shape).
    /// For a printable single key it's just `"a"`; for non-printable
    /// keys it's a spelled-out name like `"Backspace"` / `"Enter"` /
    /// `"Up"` so the burst can spell them out as multiple particles.
    /// Modifier-only presses (shift / cmd / opt / ctrl) and unknown
    /// special keys come through as `None`. Mouse triggers always
    /// pass `None`. The label is fed straight into the burst spawner
    /// and not stored anywhere else — it lives on the callback's
    /// stack frame and dies with the burst.
    pub fn start<F>(screen_h: f32, callback: F)
    where
        F: Fn(FxTrigger, f32, f32, Option<String>) + Send + Sync + 'static,
    {
        // Read the system's double-click interval once. AppKit's
        // default is ~0.5s; users can crank it down to 0.1 (very
        // fast) or up to ~5s in System Settings → Mouse → Tracking.
        let double_interval_s = unsafe { read_double_click_interval() };
        let state = std::sync::Arc::new(Mutex::new(ClickState {
            last_left: None,
            double_interval_s,
        }));
        let callback = std::sync::Arc::new(callback);
        let block = {
            let state = state.clone();
            let callback = callback.clone();
            RcBlock::new(move |event: *mut AnyObject| {
                if event.is_null() {
                    return;
                }
                let ty: u64 = unsafe { msg_send![event, type] };
                let (x, y) = unsafe { current_cursor_topleft(screen_h) };
                match ty {
                    NSEVENT_TYPE_KEY_DOWN => {
                        let label = unsafe { read_typed_label(event) };
                        callback(FxTrigger::Key, x, y, label);
                    }
                    NSEVENT_TYPE_RIGHT_MOUSE_DOWN => {
                        callback(FxTrigger::MouseRight, x, y, None);
                    }
                    NSEVENT_TYPE_LEFT_MOUSE_DOWN => {
                        // Always fire the single-click burst first.
                        // Delaying until we know whether a 2nd click
                        // is coming would lag every click by the
                        // doubleClickInterval (~500ms) — unusable.
                        callback(FxTrigger::Mouse, x, y, None);
                        // Then check if this completes a double.
                        let mut st = match state.lock() {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                        let now = Instant::now();
                        let mut is_double = false;
                        if let Some((t, px, py)) = st.last_left {
                            let dt = now.duration_since(t).as_secs_f32();
                            let dx = x - px;
                            let dy = y - py;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dt <= st.double_interval_s
                                && dist <= DOUBLE_CLICK_DISTANCE_PX
                            {
                                is_double = true;
                            }
                        }
                        // A completed double-click resets the
                        // tracker so a third quick click doesn't
                        // chain into "triple-click reads as another
                        // double" — that would feel spammy. After a
                        // double, the next click starts a fresh
                        // single.
                        st.last_left = if is_double { None } else { Some((now, x, y)) };
                        drop(st);
                        if is_double {
                            callback(FxTrigger::MouseDouble, x, y, None);
                        }
                    }
                    _ => {}
                }
            })
        };
        // Local monitor mirrors the global handler but returns the
        // event so AppKit keeps dispatching it to the focused window
        // (sabitori click handling, settings sliders, etc.). Without
        // the return value the event would be swallowed and the modal
        // wouldn't react to any input.
        let local_block = {
            let state = state.clone();
            let callback = callback.clone();
            RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                if !event.is_null() {
                    let ty: u64 = unsafe { msg_send![event, type] };
                    let (x, y) = unsafe { current_cursor_topleft(screen_h) };
                    match ty {
                        NSEVENT_TYPE_KEY_DOWN => {
                            let label = unsafe { read_typed_label(event) };
                            callback(FxTrigger::Key, x, y, label);
                        }
                        NSEVENT_TYPE_RIGHT_MOUSE_DOWN => {
                            callback(FxTrigger::MouseRight, x, y, None);
                        }
                        NSEVENT_TYPE_LEFT_MOUSE_DOWN => {
                            callback(FxTrigger::Mouse, x, y, None);
                            if let Ok(mut st) = state.lock() {
                                let now = Instant::now();
                                let mut is_double = false;
                                if let Some((t, px, py)) = st.last_left {
                                    let dt = now.duration_since(t).as_secs_f32();
                                    let dx = x - px;
                                    let dy = y - py;
                                    let dist = (dx * dx + dy * dy).sqrt();
                                    if dt <= st.double_interval_s
                                        && dist <= DOUBLE_CLICK_DISTANCE_PX
                                    {
                                        is_double = true;
                                    }
                                }
                                st.last_left = if is_double { None } else { Some((now, x, y)) };
                                drop(st);
                                if is_double {
                                    callback(FxTrigger::MouseDouble, x, y, None);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                event
            })
        };
        unsafe {
            let event_cls = AnyClass::get("NSEvent").expect("NSEvent");
            let mask: u64 = NSEVENT_MASK_LEFT_MOUSE_DOWN
                | NSEVENT_MASK_RIGHT_MOUSE_DOWN
                | NSEVENT_MASK_KEY_DOWN;
            // AppKit copies + retains the block when the monitor is
            // installed; our local `RcBlock` can drop at end of scope
            // without invalidating the registration.
            let _global_token: *mut AnyObject = msg_send![
                event_cls,
                addGlobalMonitorForEventsMatchingMask: mask,
                handler: &*block,
            ];
            let _local_token: *mut AnyObject = msg_send![
                event_cls,
                addLocalMonitorForEventsMatchingMask: mask,
                handler: &*local_block,
            ];
        }
    }

    unsafe fn current_cursor_topleft(screen_h: f32) -> (f32, f32) {
        let event_cls = AnyClass::get("NSEvent").expect("NSEvent");
        let loc: NSPoint = msg_send![event_cls, mouseLocation];
        let x = loc.x as f32;
        // macOS reports cursor Y from the bottom; flip to top-left
        // origin so the fx overlay's sabitori coordinate space matches.
        let y = screen_h - loc.y as f32;
        (x, y)
    }

    /// Pull a printable burst label out of an NSEvent keyDown.
    /// `[NSEvent characters]` returns the post-modifier string the OS
    /// would type into a focused field — already maps shift / option
    /// layouts correctly. For printable ordinary keys we just return
    /// that string verbatim. For control codes (\u{8}, \u{D}, …) and
    /// AppKit's private-use codepoints (arrows / function / nav keys
    /// in F700–F7FF) we substitute a spelled-out name so the burst
    /// can render them as multiple glyph particles ("Backspace" → 9
    /// particles cycling B a c k s p a c e). Unknown special keys
    /// return `None` so the burst falls back to the configured shape.
    unsafe fn read_typed_label(event: *mut AnyObject) -> Option<String> {
        let ns_str: *mut AnyObject = msg_send![event, characters];
        if ns_str.is_null() {
            return None;
        }
        let cstr: *const std::os::raw::c_char = msg_send![ns_str, UTF8String];
        if cstr.is_null() {
            return None;
        }
        // 2024 edition: unsafe fn 内でも CStr::from_ptr の呼び出しには明示
        // unsafe ブロックが要る (`unsafe_op_in_unsafe_fn` lint)。
        let s = unsafe { std::ffi::CStr::from_ptr(cstr) }.to_str().ok()?;
        let c = s.chars().next()?;
        // Special-key map. Names follow the keycap convention so the
        // particles read as the user expects ("Esc" not "Escape" so
        // the burst stays compact; "PgUp" not "PageUp" same reason).
        let named = match c {
            // Whitespace / common control. Space normally renders as
            // an invisible glyph — give it a name so users see
            // *something* fly when they hit space.
            ' ' => Some("Space"),
            // `\t` is U+0009, same as `\u{9}` — keep just one arm.
            '\t' => Some("Tab"),
            '\u{D}' | '\u{A}' => Some("Enter"),
            '\u{1B}' => Some("Esc"),
            '\u{8}' | '\u{7F}' => Some("Backspace"),
            // AppKit private-use area for arrow / nav / function keys.
            '\u{F700}' => Some("Up"),
            '\u{F701}' => Some("Down"),
            '\u{F702}' => Some("Left"),
            '\u{F703}' => Some("Right"),
            '\u{F728}' => Some("Del"),
            '\u{F729}' => Some("Home"),
            '\u{F72B}' => Some("End"),
            '\u{F72C}' => Some("PgUp"),
            '\u{F72D}' => Some("PgDn"),
            _ => None,
        };
        if let Some(name) = named {
            return Some(name.to_string());
        }
        // F-keys F1..F12 → derive name from offset past F704.
        if ('\u{F704}'..='\u{F70F}').contains(&c) {
            let n = c as u32 - 0xF704 + 1;
            return Some(format!("F{}", n));
        }
        // Anything else still in the private-use area is a special
        // key we haven't named — skip rather than render a missing
        // glyph.
        if ('\u{F700}'..='\u{F8FF}').contains(&c) {
            return None;
        }
        // Other control codes we didn't enumerate (rare) — skip.
        if c.is_control() {
            return None;
        }
        // Printable single key. Return just the first char so combos
        // like accented "é" come through as one particle, not the
        // raw composed string.
        Some(c.to_string())
    }

    unsafe fn read_double_click_interval() -> f32 {
        let event_cls = AnyClass::get("NSEvent").expect("NSEvent");
        let interval: f64 = msg_send![event_cls, doubleClickInterval];
        // Defensive clamp: a misbehaving system pref shouldn't let
        // a 0s interval cause every click to count as a double, or a
        // 60s interval mean clicks separated by ~a minute amplify.
        (interval as f32).clamp(0.05, 2.0)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{start, FxTrigger};

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxTrigger {
    Mouse,
    MouseRight,
    MouseDouble,
    Key,
}

#[cfg(not(target_os = "macos"))]
pub fn start<F>(_screen_h: f32, _: F)
where
    F: Fn(FxTrigger, f32, f32, Option<String>) + Send + Sync + 'static,
{
}
