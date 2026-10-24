# Building MCDB

MCDB is a Rust desktop app (`eframe`/`egui`, bundled SQLite, `native-tls`). It builds
natively on Windows, macOS, and Linux; each platform needs its own C/C++ toolchain
because SQLite and the TLS backend are compiled from C.

The version comes from `Cargo.toml`; the minimum macOS version is declared in
`macos/Info.plist` (`LSMinimumSystemVersion`).

| Platform | Command | Output |
| --- | --- | --- |
| Windows | `cargo build --release` | `target\release\ModDB.exe` |
| Windows installer | `makensis installer.nsi` | `dist\MCDB-1.1-release-opensource.exe` |
| macOS | `bash macos/build.sh` | `dist/macos/ModDB.app` + `dist/macos/MCDB-1.1.0-macos-universal.zip` |
| Linux | `cargo build --release` | `target/release/ModDB` |

The Python files (`db.py`, `providers.py`, `server.py`, `compat.py`) and the `static/`
and `data/` folders are leftovers from an earlier prototype. The Rust app does not
use them.

## Windows

### Prerequisites

- **Rust (MSVC toolchain)** via [rustup](https://rustup.rs). `rustup show` should list
  `stable-x86_64-pc-windows-msvc`, which is what this project is built and tested with;
  the GNU toolchain is untested here.
- **Visual Studio Build Tools 2022 or newer** with the **Desktop development with C++**
  workload (MSVC compiler, Windows SDK, `link.exe`). The full Visual Studio IDE is not
  required, but the C++ workload is.
- **NSIS 3** only if you want the installer. The default install path is
  `C:\Program Files (x86)\NSIS`; add it to `PATH` or call `makensis` by full path.

### Build

```bat
cargo build --release
target\release\ModDB.exe
```

`build.rs` embeds `assets/icon.ico` into the executable on Windows only, using
`embed-resource`, so the `.exe` carries its icon without a separate resource file.

The first build downloads and compiles every crate and can take a few minutes. The
release profile uses thin LTO, so rebuilds of the final binary are slower than debug
builds; use `cargo run` while iterating.

### Installer

The NSIS script packages the already-built executable, so build first, then:

```bat
if not exist dist mkdir dist
makensis installer.nsi
```

`dist\` is gitignored, so a fresh clone does not contain it and `makensis` fails with
`Error: opening file for writing "dist\..."` until you create the folder. The script
also hardcodes `target\release\ModDB.exe`, so it will not pick up a debug build.

Do not try to run `macos/build.sh` through the `bash` command in PowerShell: that
launches WSL. Unless a Linux distribution is installed, it fails with
`WSL ERROR: CreateProcessCommon: execvpe(/bin/bash) failed`, and even with one it
cannot produce a macOS build. Git Bash runs the script correctly but it intentionally
refuses to build off macOS.

## macOS

### Prerequisites

- macOS 11 or newer. macOS Sequoia 15.x, including 15.8.1, is supported; the bundle
  declares `LSMinimumSystemVersion` 11.0.
- **Xcode Command Line Tools**: `xcode-select --install`. This provides `clang`,
  `lipo`, `codesign`, `sips`, and `iconutil`. The full Xcode app is not needed.
- **Rust via rustup**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
  Homebrew's `rust` formula does not include `rustup`, which `build.sh` needs in order
  to add the two Apple targets.
- An Apple Silicon or Intel Mac both work. The script cross-compiles the other slice.

### Build

```bash
bash macos/build.sh
```

Use `bash macos/build.sh` rather than `./macos/build.sh`: Git does not store the
executable bit for this file, so the direct form can fail with `Permission denied`
after a fresh clone or unzip.

What the script does:

1. Reads the version from `Cargo.toml`.
2. Adds `aarch64-apple-darwin` and `x86_64-apple-darwin` and builds release binaries
   for both.
3. `lipo`s them into one universal executable inside `dist/macos/ModDB.app`.
4. Copies `macos/Info.plist`, stamps `CFBundleShortVersionString`/`CFBundleVersion`
   from `Cargo.toml`, and builds `icon.icns` from `assets/icon.png`.
5. Ad-hoc signs the bundle (`codesign --force --deep --sign -`) and verifies it.
6. Zips the app with `ditto`, preserving it correctly for distribution.

Set `UNIVERSAL=0` to build only the current Mac's architecture, which halves the build
time:

```bash
UNIVERSAL=0 bash macos/build.sh
```

### Run and install

```bash
open dist/macos/ModDB.app
ditto dist/macos/ModDB.app /Applications/ModDB.app
```

A locally built app is not quarantined, so it launches normally. If you copy the zip to
another Mac, the app is ad-hoc signed but not notarized: right-click → **Open** the
first time, or run `xattr -dr com.apple.quarantine /Applications/ModDB.app`.

Shipping it broadly without Gatekeeper warnings needs an Apple Developer ID
certificate and notarization, which this project does not currently do.

### Checking the bundle

```bash
file dist/macos/ModDB.app/Contents/MacOS/ModDB   # Mach-O universal binary with 2 architectures
codesign -dv dist/macos/ModDB.app                # Signature=adhoc
plutil -p dist/macos/ModDB.app/Contents/Info.plist | grep -E 'ShortVersion|Minimum'
```

The macOS build cannot be produced from Windows or Linux: Apple's SDK, linker, and
`codesign` are required. Cross-compiling with the installed `x86_64-apple-darwin`
target fails in C dependencies such as `ring` with errors like
`unrecognized command-line option '-arch'`.

## Linux

### Prerequisites

- **Rust via rustup** (distro packages work only if they ship the full toolchain).
- **C/C++ toolchain**: `build-essential` on Debian/Ubuntu, `base-devel` on Arch,
  `gcc`/`make` elsewhere. Needed to compile the C dependencies (bundled SQLite and
  `ring`).
- **OpenSSL development headers and `pkg-config`**: `libssl-dev pkg-config` on
  Debian/Ubuntu, `openssl-devel` on Fedora. `native-tls` links the system OpenSSL.
- A graphical session with working OpenGL/EGL (X11 or Wayland). `eframe` renders via
  `glow`; a software Mesa driver is enough, a display is not optional.
- `xdg-utils` at runtime for "open link" and "open folder" actions. Without `xdg-open`
  those buttons silently do nothing; the rest of the app still works.

### Build and run

```bash
cargo build --release
./target/release/ModDB
```

There is no Linux packaging script. For distribution you would add your own
AppImage/Flatpak/tarball packaging; only the macOS bundle and the Windows NSIS
installer exist in this repo.

## What a successful build looks like

- Windows: `target\release\ModDB.exe` is about 12 MB and launches a native window.
- macOS: the checks above report a universal binary with an ad-hoc signature.
- Linux: `target/release/ModDB` launches; if the window never appears, check the
  display/OpenGL notes below.

A quick functional smoke test that needs no API key: type `optimization` in the Find
box, press **Find**, and confirm Modrinth results appear. CurseForge results need its
API key (a default key is compiled in; you can override it in **Settings**).

## Common failures

### Windows

| Symptom | Cause and fix |
| --- | --- |
| `error: linker 'link.exe' not found` | Missing MSVC C++ workload or Windows SDK. Install Visual Studio Build Tools with "Desktop development with C++", then reopen the terminal. |
| `error: failed to run custom build command for 'libsqlite3-sys'` | Same missing C compiler/SDK; bundled SQLite is compiled with `cc`. |
| `error: linking with 'cc' failed` / non-MSVC linker errors | A non-MSVC toolchain is active or the MSVC tools are missing. Run `rustup default stable-x86_64-pc-windows-msvc` and reopen the terminal. |
| `makensis: command not found` | NSIS is not installed or not on `PATH`. |
| `Error: opening file for writing "dist\..."` | Create the folder first: `if not exist dist mkdir dist`. |
| PowerShell prints `WSL ERROR: CreateProcessCommon: execvpe(/bin/bash) failed` | `bash` in PowerShell is the WSL launcher. This has nothing to do with building MCDB; macOS builds still require a Mac. |

### macOS

| Symptom | Cause and fix |
| --- | --- |
| `xcrun: error: invalid active developer path` | Command Line Tools are missing or broken: `xcode-select --install`, or `sudo xcode-select -s /Library/Developer/CommandLineTools` if they are already installed. |
| `error: linker 'cc' not found` | Same missing Command Line Tools. |
| `sips: command not found` / `iconutil: command not found` | Command Line Tools missing. The app still builds, but without an `.icns` icon. |
| `./macos/build.sh: Permission denied` | The executable bit is not tracked by Git. Use `bash macos/build.sh`. |
| `rustup: command not found` | Rust came from Homebrew. Install rustup; `build.sh` needs `rustup target add`. |
| `codesign` fails with `resource fork, Finder information, or similar detritus not allowed` | Extended attributes on the bundle. `xattr -cr dist/macos/ModDB.app`, then rerun the script. |
| "cannot be opened because the developer cannot be verified" | Ad-hoc signature and quarantine on a downloaded copy: right-click → Open, or `xattr -dr com.apple.quarantine ModDB.app`. |
| "is damaged and can't be opened" | Same quarantine issue on an unnotarized app; the command above fixes it. A locally built copy is unaffected. |
| `error: unrecognized command-line option '-arch'` while targeting Apple from Windows/Linux | Cross-compiling is not supported. Build on a Mac or use the macOS CI workflow. |

### Linux

| Symptom | Cause and fix |
| --- | --- |
| `Could not find directory of OpenSSL installation` | Install `libssl-dev` (or `openssl-devel`) and `pkg-config`, then clean and rebuild. |
| `error: linker 'cc' not found` | Install `build-essential`/`gcc`. |
| `error while loading shared libraries: libssl.so.3` | The runtime OpenSSL package is missing; install your distro's `libssl3`/`openssl-libs`. |
| `Failed to initialize any backend` / no window on a server or WSL | The app needs an X11 or Wayland display. Run it in a desktop session, not over plain SSH. |
| Window opens but is blank or crashes at startup | GPU/OpenGL driver issue. Try a software Mesa driver (`LIBGL_ALWAYS_SOFTWARE=1`). |
| "Open folder" / link buttons do nothing | Install `xdg-utils` so `xdg-open` exists. |

### Any platform

| Symptom | Cause and fix |
| --- | --- |
| `Blocking waiting for file lock on build directory` | Another `cargo` process is using `target/`. Wait for it or use a separate `CARGO_TARGET_DIR`. |
| `failed to select a version for ...` after editing `Cargo.toml` | Cargo and `Cargo.lock` disagree. Restore the lockfile or run `cargo update` deliberately; do not delete the lock for a release build. |
| Build appears to hang during the final link | Thin LTO is enabled in the release profile; the last step takes a while. |
| App starts but cannot reach Modrinth/CurseForge | Network or proxy issue; errors are written to `error.log` in the data folder below. |

## Building on GitHub (CI)

`.github/workflows/macos.yml` builds the universal `.app` on a GitHub-hosted macOS
Sequoia runner. It runs on manual dispatch and on `v*` tags, and uploads
`MCDB-macos` (the zip) as a build artifact. It does not create a GitHub Release, and
there are no Windows or Linux CI jobs.

This only works from a GitHub repository you can push to. The local `origin` currently
points at `https://github.com/NeofastFTL-Dev/MCModDatabase.git`, and that URL is
publicly readable (it answers with `main` and the `v1.0.0`/`v1.1.0` tags), so make
sure it is the repository you actually want to use. If you are starting without a
repository, create one first.

### 1. Create the repository (skip if it already exists)

On github.com: **New repository** → name `MCModDatabase` → **Public** → do not add a
README, `.gitignore`, or license → **Create repository**. An empty repository avoids a
non-fast-forward rejection on the first push.

Or with the GitHub CLI:

```bash
gh auth login
gh repo create NeofastFTL-Dev/MCModDatabase --public
```

Use `--private` instead of `--public` if you prefer. Public repositories get free
macOS runner minutes; private ones bill macOS at 10× the normal per-minute rate. If you
create it under a different account or organization, substitute that owner everywhere.

### 2. Point `origin` at your repository

```bash
git remote -v
git remote set-url origin https://github.com/<owner>/<repo>.git
```

Only needed when the existing URL is not the repository you will push to. For SSH use
`git@github.com:<owner>/<repo>.git`.

### 3. Commit and push the build changes

```bash
git add .github/workflows/macos.yml BUILD.md README.md macos/build.sh macos/Info.plist src/update.rs src/gui.rs
git commit -m "Build universal MCDB app for macOS"
git push -u origin main
```

Stage those paths deliberately; `git add -A` would also pick up unrelated untracked
files. The workflow must be committed together with `macos/build.sh`, otherwise CI
builds the old non-universal bundle. If the push is rejected because the GitHub
repository already has commits, run `git pull --rebase origin main` and push again.

### 4. Run the workflow

GitHub → your repository → **Actions** → **macOS build** → **Run workflow** → branch
`main`. The button only appears once the workflow file is on the default branch. If
Actions is disabled, enable it under **Settings → Actions → General**. Pushing a `v*`
tag also triggers a build.

### 5. Download the app

The run takes roughly 5–10 minutes. Open the finished run and download the
`MCDB-macos` artifact (kept for 30 days), then unzip it to find
`MCDB-1.1.0-macos-universal.zip` and unzip that for `ModDB.app`.

Artifact downloads require being signed in to GitHub, so this is not a public download
link. Attaching the zip to a Release is a separate step that the workflow does not do.

## App data and settings

Settings live in one `settings.ini` file, next to `error.log`, under:

- Windows: `%LOCALAPPDATA%\MCModDatabase`
- macOS: `~/Library/Application Support/MCModDatabase`
- Linux: `$XDG_DATA_HOME/MCModDatabase` (or `~/.local/share/MCModDatabase`)

Downloads default to `~/Downloads/MCMods` on every platform and can be changed in
**Settings**. Deleting the data folder resets settings and the cached catalog.
