//! Bundled fonts (JetBrains Mono + Nerd Font symbols), registered as
//! process-private fonts at startup so the terminal and editor resolve
//! them even when they aren't installed system-wide.

/// Register every bundled font for this process. On Windows the fonts are
/// added from memory via `AddFontMemResourceEx` (memory fonts are private
/// by design and removed automatically when the process exits); elsewhere
/// this is a no-op.
pub fn register_fonts() {
    #[cfg(windows)]
    imp::register();
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Graphics::Gdi::AddFontMemResourceEx;

    const FONTS: &[(&str, &[u8])] = &[
        (
            "JetBrainsMono-Regular.ttf",
            include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
        ),
        (
            "JetBrainsMono-Bold.ttf",
            include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf"),
        ),
        (
            "SymbolsNerdFont-Regular.ttf",
            include_bytes!("../../assets/fonts/SymbolsNerdFont-Regular.ttf"),
        ),
    ];

    pub fn register() {
        for (name, data) in FONTS {
            let count: u32 = 0;
            let handle = unsafe {
                AddFontMemResourceEx(
                    data.as_ptr() as *const core::ffi::c_void,
                    data.len() as u32,
                    None,
                    &count,
                )
            };
            if handle.0.is_null() {
                log::warn!("failed to register bundled font {name}");
            }
        }
    }
}
