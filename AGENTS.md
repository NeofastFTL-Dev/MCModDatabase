# MCDB Agents guide

MCDB exposes your **local Rust catalog** (`catalog.db` under the app data dir) to AI tools via:

1. **CLI** — `ModDB agent …` (no server)
2. **Localhost JSON API** — `http://127.0.0.1:8765` with a Bearer API key

There is **no cloud OAuth / ChatGPT plugin** yet. Tools that can call HTTP on your machine or run a shell command work out of the box. Browser-only ChatGPT/Claude need a desktop bridge, MCP wrapper, or you paste CLI output.

In the app, open the **Agents** tab to generate a key, start the API, and copy examples.

## Quick start

```bat
ModDB.exe agent key generate
ModDB.exe agent serve --port 8765
```

Or use **Agents → Start local API** in the GUI.

Default data dir (catalog + settings):

- Windows: `%LOCALAPPDATA%\MCModDatabase\`
- macOS: `~/Library/Application Support/MCModDatabase/`

## Authentication

All endpoints except `GET /health` require:

```http
Authorization: Bearer <your-api-key>
```

Create/show the key:

```bat
ModDB.exe agent key generate
ModDB.exe agent key show
```

Or use the **Agents** tab (Generate / Copy / Regenerate).

## HTTP API (127.0.0.1 only)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | no | Liveness |
| GET | `/status` | yes | Catalog + library counts, version |
| GET | `/search?q=&limit=&source=&loader=&version=&type=` | yes | Full-text catalog search |
| GET | `/library` | yes | Saved library mods |
| GET | `/mods/{source}/{id-or-slug}` | yes | One mod (`modrinth` / `curseforge`) |

Example:

```bash
curl -s -H "Authorization: Bearer YOUR_KEY" \
  "http://127.0.0.1:8765/search?q=sodium&limit=5"
```

## CLI

```bat
ModDB.exe agent status --json
ModDB.exe agent search "sodium" --json --limit 10 --loader fabric --version 1.21.1
ModDB.exe agent get modrinth AANobbMI --json
ModDB.exe agent library --json
ModDB.exe agent serve --port 8765
```

## Connecting common AI tools

### Cursor / Grok Bot / Claude Code / local coding agents

- Prefer CLI: tell the agent to run `ModDB.exe agent search "…" --json` (full path to the installed binary if needed).
- Or start the local API and let the agent `curl` / `fetch` `http://127.0.0.1:8765/...` with the Bearer header.

### Freebuff / custom MCP / HTTP tool wrappers

Configure:

- **Base URL:** `http://127.0.0.1:8765`
- **Header:** `Authorization: Bearer <key from Agents tab>`
- Map tools to `/search`, `/get` via `/mods/{source}/{id}`, `/library`, `/status`

### ChatGPT / Claude (web)

These cannot reach your localhost by default. Options:

1. Use a desktop client or MCP bridge that can call local HTTP.
2. Run CLI commands yourself and paste JSON into the chat.
3. Expose via your own tunnel only if you understand the risk (API key = full catalog read). Prefer localhost.

### Grok (xAI apps)

Same as other agents: local HTTP or CLI when the product allows tool/shell use. Point tools at `127.0.0.1` + Bearer key from the Agents tab.

## What is / isn’t connected out of the box

| Capability | Status |
|------------|--------|
| Local CLI JSON for any agent that can run shell | Yes |
| Localhost Bearer API for HTTP-capable agents | Yes |
| In-app Agents tab (key + start/stop + docs) | Yes |
| Official ChatGPT / Claude / Freebuff marketplace OAuth plugin | Not in this release — use HTTP/CLI config above |

## Version

Documented for MCDB **1.3.0**.
