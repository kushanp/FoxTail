use egui::IconData;

include!("icon_pixels.rs");

/// Fox-head icon for the window chrome. The same artwork is embedded in the
/// .exe as a Windows icon resource by build.rs.
pub fn app_icon() -> IconData {
    // Draw at 4x and box-filter down for antialiased edges.
    let (big, rgba) = render_fox(4);
    let (size, rgba) = downsample(&rgba, big, 4);
    IconData {
        rgba,
        width: size,
        height: size,
    }
}
