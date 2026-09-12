#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod icon;
mod theme;

fn main() {
    match cli::parse_args(std::env::args_os().skip(1)) {
        cli::Action::Help => {
            emit_cli_text(&cli::help_text(&exe_name()));
            return;
        }
        cli::Action::Version => {
            emit_cli_text(&cli::version_text());
            return;
        }
        cli::Action::Run { files } => {
            if let Err(err) = app::run(files) {
                let message = format_startup_error(&err);
                eprintln!("{message}");
                show_error_dialog(&message);
                std::process::exit(1);
            }
        }
    }
}

fn exe_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "foxtail.exe".into())
}

fn emit_cli_text(text: &str) {
    // Debug builds use the console subsystem, so stdout is already connected.
    #[cfg(not(all(windows, not(debug_assertions))))]
    {
        print!("{text}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }

    // Release Windows builds are GUI-subsystem: attach the parent console
    // (cmd/PowerShell) so --help / --version are visible. If there is no
    // console, fall back to a message box.
    #[cfg(all(windows, not(debug_assertions)))]
    {
        if !emit_windows_console(text) {
            windows_message_box("FoxTail", text.trim_end(), MB_ICONINFORMATION);
        }
    }
}

#[cfg(all(windows, not(debug_assertions)))]
fn emit_windows_console(text: &str) -> bool {
    use std::io::Write;

    unsafe {
        const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(dw_process_id: u32) -> i32;
        }
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    match std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        Ok(mut console) => console.write_all(text.as_bytes()).is_ok(),
        Err(_) => false,
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
    windows_message_box("FoxTail", message, MB_ICONERROR);

    #[cfg(not(windows))]
    let _ = message;
}

#[cfg(windows)]
const MB_OK: u32 = 0x0000_0000;
#[cfg(windows)]
const MB_ICONERROR: u32 = 0x0000_0010;
#[cfg(all(windows, not(debug_assertions)))]
const MB_ICONINFORMATION: u32 = 0x0000_0040;
#[cfg(windows)]
const MB_SETFOREGROUND: u32 = 0x0001_0000;

#[cfg(windows)]
fn windows_message_box(title: &str, message: &str, icon: u32) {
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
            MB_OK | icon | MB_SETFOREGROUND,
        );
    }
}
