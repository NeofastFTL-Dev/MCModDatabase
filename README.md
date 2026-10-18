<p align="center">
  <img src="assets/icon.png" width="96" alt="MCDB icon">
</p>

<h1 align="center">MCDB</h1>

<p align="center">
  Desktop search for <b>Minecraft mods</b> on CurseForge and Modrinth.<br>
  Describe what you want, browse a local catalog, then download a pack to a folder or zip.
</p>

<p align="center">
  <a href="https://github.com/NeofastFTL-Dev/MCModDatabase/releases/tag/v1.1.0"><img src="https://img.shields.io/badge/download-MCDB--1.1--release--opensource-3ee0a0?style=for-the-badge" alt="Download"></a>
  <img src="https://img.shields.io/badge/license-MIT-informational?style=for-the-badge" alt="MIT">
</p>

## What it does

MCDB is a Windows app (ModDB) that talks to **Modrinth** and **CurseForge** so you can find mods without opening a browser. Type a normal sentence, filter by loader and Minecraft version, then pick the jars you want.

![Find prompt searching lighting mods](docs/search.png)

**Find** understands prompts like `fabric 1.21.1 light rendering plus colorful lightning`. It pulls the version and loader out of the text, ranks name matches first (so mods like Colorful Lighting show up), and mixes live API results with the local catalog.

![Local catalog by theme](docs/catalog.png)

**Catalog** is a local cache of the most-downloaded projects per source and type. Browse by theme (lighting, optimization, magic, tech, …) even when you are offline. It rebuilds in the background in minutes, not days.

![Selecting mods and downloading a zip](docs/download.png)

Select several mods, then **Download to folder** or **Save zip**. MCDB resolves matching files for your loader/version, writes the jars where you choose, or packs them into one zip.

## Features

- Search CurseForge + Modrinth together, or one site at a time
- Natural-language Find prompt (loader, version, themes, mod names)
- Local catalog with theme browse and full-text search
- **Pack** - describe a modpack and I'll pick compatible mods for your loader/version
- Compatibility check on a multi-mod selection
- Download to a folder or a zip
- Right-click a mod for its description, links, and GitHub issues when a repo exists
- Mods, modpacks, resource packs, and shaders

## Install

1. Grab [**MCDB-1.1-release-opensource.exe**](https://github.com/NeofastFTL-Dev/MCModDatabase/releases/tag/v1.1.0)
2. Run the installer (installs to `Program Files\MCDB` and adds Start Menu + desktop shortcuts)
3. Optional: in **Settings**, paste a [CurseForge API key](https://console.curseforge.com) if you want CurseForge results (Modrinth works without one)

## Use it

1. Pick **Pack**, describe the pack you want, then **Build pack**
2. Type what you need in the top box and press **Find** (or Ctrl+Enter)
3. Tick the mods to keep
4. **Download to folder** or **Save zip**
5. Open **Catalog** to browse the cached library by theme

## Build from source

```bat
cargo build --release
```

Windows installer:

```bat
makensis installer.nsi
```

Output: `dist\MCDB-1.1-release-opensource.exe`

macOS app bundle: `macos/build.sh` (must run on a Mac).

## License

[MIT](LICENSE) © NeofastFTL
