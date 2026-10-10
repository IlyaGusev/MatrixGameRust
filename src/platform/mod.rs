#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
mod web;

pub mod audio;

#[cfg(target_arch = "wasm32")]
pub mod audio_web;

#[cfg(not(target_arch = "wasm32"))]
pub mod audio_native;

pub mod sound_pkg_map;

#[cfg(not(target_arch = "wasm32"))]
pub fn now_secs() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
}

#[cfg(target_arch = "wasm32")]
pub fn now_secs() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now() / 1000.0
}

/// Update the on-screen FPS overlay (`#fps` div in index.html). No-op on
/// native (the FPS is logged to the console there instead).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_fps_text(_text: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn set_fps_text(text: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("fps"))
    {
        el.set_inner_html(text);
    }
}
