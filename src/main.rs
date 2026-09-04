#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icon;
mod theme;

fn main() {
    if let Err(err) = app::run() {
        let message = format_startup_error(&err);
        eprintln!("{message}");
        show_error_dialog(&message);
        std::process::exit(1);
    }
}

fn format_startup_error(err: &eframe::Error) -> String {
    let details = err.to_string();
    let lower = details.to_ascii_lowercase();
    let gpu_failure = lower.contains("wgpu")
        || lower.contains("adapter")
        || lower.contains("graphics")
        || lower.contains("opengl")
        || lower.contains("vulkan")
        || lower.contains("dx12")
        || lower.contains("surface");

    if gpu_failure {
        format!(
            "FoxTail could not start because this PC has no usable graphics adapter.\n\n\
             FoxTail needs DirectX 12 on Windows 10 or 11, with a GPU (integrated is fine) \
             or a VM that exposes one. Software-only VMs and some remote-desktop sessions will fail.\n\n\
             Details:\n{details}"
        )
    } else {
        format!("FoxTail could not start.\n\n{details}")
    }
}

fn show_error_dialog(message: &str) {
    #[cfg(windows)]
    windows_message_box("FoxTail", message);

    #[cfg(not(windows))]
    let _ = message;
}

#[cfg(windows)]
fn windows_message_box(title: &str, message: &str) {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            hwnd: *mut core::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            utype: u32,
        ) -> i32;
    }

    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONERROR: u32 = 0x0000_0010;
    const MB_SETFOREGROUND: u32 = 0x0001_0000;

    let text: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let caption: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}
