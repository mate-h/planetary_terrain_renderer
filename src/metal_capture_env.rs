#[cfg(all(feature = "metal_capture", target_vendor = "apple"))]
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static INIT_METAL_CAPTURE: extern "C" fn() = init_metal_capture_env;

#[cfg(all(feature = "metal_capture", target_vendor = "apple"))]
extern "C" fn init_metal_capture_env() {
    // SAFETY: runs before main, before Metal device creation.
    unsafe { std::env::set_var("METAL_CAPTURE_ENABLED", "1") };
}

/// Enables programmatic Metal GPU capture.
///
/// On macOS with the `metal_capture` feature, this is called automatically via a
/// pre-main initializer. It can also be invoked manually at the start of `main`
/// before [`bevy::app::App::new`].
#[inline]
pub fn prepare_metal_capture() {
    #[cfg(all(feature = "metal_capture", target_vendor = "apple"))]
    init_metal_capture_env();
}
