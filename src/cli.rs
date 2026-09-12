use std::ffi::OsStr;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Help,
    Version,
    Run { files: Vec<PathBuf> },
}

/// Parse command-line arguments after argv[0].
///
/// `--help` / `-h` win over `--version` / `-V`. Any other argument is treated
/// as a file path, including unknown flags, matching previous behaviour.
pub fn parse_args<I, T>(args: I) -> Action
where
    I: IntoIterator<Item = T>,
    T: AsRef<OsStr>,
{
    let mut files = Vec::new();
    let mut help = false;
    let mut version = false;

    for arg in args {
        let arg = arg.as_ref();
        if arg == "--help" || arg == "-h" {
            help = true;
        } else if arg == "--version" || arg == "-V" {
            version = true;
        } else {
            files.push(PathBuf::from(arg));
        }
    }

    if help {
        Action::Help
    } else if version {
        Action::Version
    } else {
        Action::Run { files }
    }
}

pub fn version_text() -> String {
    format!(
        "FoxTail {version}\n{description}\nhttps://github.com/kushanp/FoxTail\n",
        version = env!("CARGO_PKG_VERSION"),
        description = env!("CARGO_PKG_DESCRIPTION"),
    )
}

pub fn help_text(exe: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let description = env!("CARGO_PKG_DESCRIPTION");
    format!(
        "\
FoxTail {version}
{description}

Usage:
  {exe} [OPTIONS] [FILE]...

Arguments:
  [FILE]...        Log files to open, each in its own tab

Options:
  -h, --help       Print this help and exit
  -V, --version    Print version information and exit

Follow tail, filters, find, encodings, and highlight rules are
configured in the GUI (press F1 for keyboard shortcuts).

Examples:
  {exe}
  {exe} C:\\logs\\app.log
  {exe} C:\\logs\\app.log C:\\logs\\error.log

Files can also be opened from File > Open, the recent-files list,
or by dropping them onto the window.

Config: %APPDATA%\\FoxTail\\config.json
        foxtail.json next to the executable or in the working directory

https://github.com/kushanp/FoxTail
License: MIT
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args_run_with_no_files() {
        match parse_args(&[] as &[&str]) {
            Action::Run { files } => assert!(files.is_empty()),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn file_paths_are_collected() {
        match parse_args(["C:\\logs\\a.log", "b.log"]) {
            Action::Run { files } => {
                assert_eq!(files, [PathBuf::from("C:\\logs\\a.log"), PathBuf::from("b.log")]);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn help_flag_wins() {
        assert_eq!(parse_args(["--help"]), Action::Help);
        assert_eq!(parse_args(["-h"]), Action::Help);
        assert_eq!(parse_args(["app.log", "--help"]), Action::Help);
        assert_eq!(parse_args(["--version", "-h"]), Action::Help);
    }

    #[test]
    fn version_flag() {
        assert_eq!(parse_args(["--version"]), Action::Version);
        assert_eq!(parse_args(["-V"]), Action::Version);
        assert_eq!(parse_args(["app.log", "-V"]), Action::Version);
    }

    #[test]
    fn unknown_flags_are_file_paths() {
        match parse_args(["--follow", "-f"]) {
            Action::Run { files } => {
                assert_eq!(files, [PathBuf::from("--follow"), PathBuf::from("-f")]);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn help_text_lists_flags_and_version() {
        let text = help_text("foxtail.exe");
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("--help"));
        assert!(text.contains("--version"));
        assert!(text.contains("foxtail.exe"));
        assert!(text.contains("[FILE]..."));
    }

    #[test]
    fn version_text_includes_crate_metadata() {
        let text = version_text();
        assert!(text.contains("FoxTail"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains(env!("CARGO_PKG_DESCRIPTION")));
    }
}
