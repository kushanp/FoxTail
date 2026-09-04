# FoxTail

A real-time log file viewer for Windows.

FoxTail follows growing log files the way `tail -f` does on Unix, with a GUI: multiple tabs, keyword highlighting, include/exclude filters, and search. It is a single native executable — no installer.

## Features

- **Follow tail** — stream new lines as they are written, no matter how fast the file grows
- **Large files** — sparse line index; does not load the whole file into RAM
- **File rotation** — truncation and replacement are detected and the view reindexes
- **Multiple files** — tabs, with an orange marker when an inactive tab has new lines
- **Highlight rules** — first-match-wins colouring, substring or regex
- **Filter tail** — include and/or exclude lines (literal or regex)
- **Find** — incremental search, regex, next/previous
- **Encodings** — UTF-8 (BOM), UTF-16 LE/BE, ANSI (Windows-1252)
- **Line endings** — CRLF, LF, and CR
- **Shared reads** — opens logs that another process is still writing
- **Portable config** — `%APPDATA%\FoxTail\config.json`, or `foxtail.json` next to the exe / in the working directory

## Build

```bat
cargo build --release
```

The binary is `target\release\foxtail.exe`.

Requires a recent Rust toolchain (1.95+). On Windows, the default `wgpu` / DirectX backend is used.

## GitHub Releases

Push a version tag and GitHub Actions builds a Windows x64 exe and attaches it to a Release:

```bat
git tag v0.1.0-alpha.1
git push origin v0.1.0-alpha.1
```

Assets:

- `FoxTail-<tag>-windows-x64.exe` — standalone executable
- `FoxTail-<tag>-windows-x64.zip` — exe plus README and sample log

Tags whose names contain `alpha`, `beta`, `rc`, or `pre` are marked as pre-releases. Example: `v0.1.0-alpha.1`. A tag like `v0.1.0` is a normal release.

## Usage

```bat
foxtail.exe
foxtail.exe C:\logs\app.log C:\logs\access.log
foxtail.exe samples\app.log
```

Open files from **File → Open**, from the recent-files list, or by dropping them onto the window.

### Keyboard

| Shortcut | Action |
| --- | --- |
| Ctrl+O | Open files |
| Ctrl+W | Close tab |
| Ctrl+Tab | Next tab |
| Ctrl+F | Find |
| F3 / Shift+F3 | Find next / previous |
| Ctrl+G | Go to line |
| Ctrl+L | Toggle follow tail |
| Ctrl+H | Highlight rules |
| Ctrl+C | Copy selection |
| Ctrl+Home / Ctrl+End | Jump to start / follow end |
| Ctrl++ / Ctrl+- | Font size |
| F5 | Reload |
| F1 | Help |

Scrolling up pauses follow. Turn **Follow tail** back on (or press Ctrl+End) to stick to the end again.

## Highlighting

Rules are evaluated top to bottom; the first match paints the whole line. Default rules colour `ERROR` / `FATAL`, `WARN`, `INFO`, `DEBUG` / `TRACE`, and common failure words. Edit them under **Highlight → Highlight rules**.

## License

MIT
