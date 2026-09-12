# FoxTail

A real-time log file viewer for Windows.

FoxTail follows growing log files the way `tail -f` does on Unix, with a GUI: multiple tabs, keyword highlighting, include/exclude filters, and search. Each release is a pair of standalone executables — no installer.

The rendering backend is chosen **at compile time**. A given `foxtail.exe` is either **wgpu** (DirectX 12) or **glow** (OpenGL), never both. `foxtail.exe --version` prints which one you have (`Renderer: wgpu` or `Renderer: glow`).

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

## Which build to run

| Build | Asset name | Needs | Notes |
| --- | --- | --- | --- |
| **wgpu** (default) | `FoxTail-<tag>-windows-x64.exe` | Windows 10/11, DirectX 12, a GPU (integrated is fine) | Larger binary. Fails on software-only VMs and some remote-desktop sessions. |
| **glow** | `FoxTail-<tag>-windows-x64-glow.exe` | Windows 10/11, a working OpenGL driver | Smaller binary. Prefer this if wgpu will not start, including VMs with Mesa / software OpenGL. |

If the window never appears, the wgpu build shows an error dialog when DirectX 12 is missing; try the glow build in that case.

## GitHub Releases

Push a version tag and GitHub Actions builds **both** Windows x64 executables and attaches them to a Release:

```bat
git tag v0.1.0-alpha.1
git push origin v0.1.0-alpha.1
```

Assets:

| File | Contents |
| --- | --- |
| `FoxTail-<tag>-windows-x64.exe` | wgpu executable |
| `FoxTail-<tag>-windows-x64.zip` | wgpu exe, README, sample log |
| `FoxTail-<tag>-windows-x64-glow.exe` | glow executable |
| `FoxTail-<tag>-windows-x64-glow.zip` | glow exe, README, sample log |

Tags whose names contain `alpha`, `beta`, `rc`, or `pre` are marked as pre-releases. Example: `v0.1.0-alpha.1`. A tag like `v0.1.0` is a normal release.

## Build

Requires a recent Rust toolchain (1.95+). Enable **exactly one** of the `wgpu` or `glow` features.

### wgpu (default, DirectX 12)

```bat
cargo build --release
```

Writes `target\release\foxtail.exe`.

To keep a wgpu exe next to a glow exe without overwriting:

```bat
cargo build-wgpu
```

Writes `target\release-wgpu\foxtail.exe` (`--profile release-wgpu --features wgpu`).

### glow (OpenGL)

Default features include wgpu, so glow builds must turn them off:

```bat
cargo build --release --no-default-features --features glow
```

Writes `target\release\foxtail.exe` (overwrites a wgpu `--release` output).

Side-by-side:

```bat
cargo build-glow
```

Writes `target\release-glow\foxtail.exe` (`--profile release-glow --no-default-features --features glow`).

`cargo build --features glow` without `--no-default-features` is an error: the two backends are mutually exclusive.

## Usage

```bat
foxtail.exe --help
foxtail.exe --version
foxtail.exe
foxtail.exe C:\logs\app.log C:\logs\access.log
foxtail.exe samples\app.log
```

| Option | Action |
| --- | --- |
| `-h`, `--help` | Print usage (includes the compiled renderer) and exit |
| `-V`, `--version` | Print version, renderer, and project URL, then exit |

Any other argument is a log file to open as a tab. Follow, filter, find, encoding, and highlight are configured in the GUI.

Open files from **File → Open**, from the recent-files list, or by dropping them onto the window.

### Keyboard

| Shortcut | Action |
| --- | --- |
| Ctrl+O | Open files |
| Ctrl+W | Close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+F | Find |
| F3 / Shift+F3 | Find next / previous |
| Esc | Close find / go-to |
| Ctrl+G | Go to line |
| Ctrl+L | Toggle follow tail |
| Ctrl+H | Highlight rules |
| Ctrl+C | Copy selection |
| Ctrl+A | Select all |
| Ctrl+Home / Ctrl+End | Jump to start / follow end |
| Ctrl++ / Ctrl+- | Font size |
| Ctrl+0 | Reset font size |
| F5 | Reload |
| F1 | Help |

Scrolling up pauses follow. Turn **Follow tail** back on (or press Ctrl+End) to stick to the end again.

## Highlighting

Rules are evaluated top to bottom; the first match paints the whole line. Default rules colour `ERROR` / `FATAL`, `WARN`, `INFO`, `DEBUG` / `TRACE`, and common failure words. Edit them under **Highlight → Highlight rules**.

## License

MIT
