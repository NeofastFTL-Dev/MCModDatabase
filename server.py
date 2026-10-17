from __future__ import annotations

import asyncio
import io
import json
import os
import re
import zipfile
from itertools import zip_longest
from pathlib import Path

from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import FileResponse, Response
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from compat import check_pack
from db import DATA_DIR, Database
from providers import (
    download_bytes,
    get_curseforge_mod,
    get_modrinth_project,
    list_game_versions,
    list_loader_versions,
    search_curseforge,
    search_modrinth,
    test_curseforge_key,
)

ROOT = Path(__file__).resolve().parent
STATIC = ROOT / "static"
CONFIG_PATH = DATA_DIR / "config.json"

db = Database()
app = FastAPI(title="ModDB")


class KeyPayload(BaseModel):
    api_key: str


class FavoritePayload(BaseModel):
    favorite: bool = True


class PackMod(BaseModel):
    source: str
    source_id: str
    name: str | None = None
    slug: str | None = None
    loaders: list[str] | None = None
    game_versions: list[str] | None = None
    icon_url: str | None = None
    website_url: str | None = None


class PackPayload(BaseModel):
    mods: list[PackMod]
    game_version: str = ""
    loader: str = ""
    loader_version: str = ""


def load_config() -> dict:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    if CONFIG_PATH.exists():
        try:
            return json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return {}
    return {}


def save_config(config: dict) -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    CONFIG_PATH.write_text(json.dumps(config, indent=2), encoding="utf-8")


DEFAULT_CURSEFORGE_KEY = "$2a$10$7.N6/h97BqTSTNjVynuU3O6eWkr5yzbVpZ1rtwrTqYmt98Rr/78zm"


def curseforge_key() -> str:
    env = os.environ.get("CURSEFORGE_API_KEY", "").strip()
    if env:
        return env
    saved = str(load_config().get("curseforge_api_key") or "").strip()
    return saved or DEFAULT_CURSEFORGE_KEY


def interleave(*groups: list[dict]) -> list[dict]:
    merged: list[dict] = []
    seen: set[tuple[str, str]] = set()
    for row in zip_longest(*groups):
        for item in row:
            if not item:
                continue
            key = (item["source"], str(item["source_id"]))
            if key in seen:
                continue
            seen.add(key)
            merged.append(item)
    return merged


@app.get("/api/status")
def status():
    return {
        "curseforge": bool(curseforge_key()),
        "stats": db.stats(),
        "recent": db.recent_searches(),
    }


@app.post("/api/settings/curseforge")
async def save_curseforge_key(payload: KeyPayload):
    key = payload.api_key.strip()
    if key:
        ok = await test_curseforge_key(key)
        if not ok:
            raise HTTPException(400, "CurseForge rejected that API key")
    config = load_config()
    config["curseforge_api_key"] = key
    save_config(config)
    return {"curseforge": bool(key)}


def _pack_mods(payload: PackPayload) -> list[dict]:
    mods = []
    for item in payload.mods:
        cached = db.get_mod(item.source.lower(), item.source_id) or {}
        mods.append(
            {
                **cached,
                "source": item.source.lower(),
                "source_id": str(item.source_id),
                "name": item.name or cached.get("name") or item.source_id,
                "slug": item.slug or cached.get("slug"),
                "loaders": item.loaders or cached.get("loaders") or [],
                "game_versions": item.game_versions or cached.get("game_versions") or [],
                "icon_url": item.icon_url or cached.get("icon_url"),
                "website_url": item.website_url or cached.get("website_url"),
            }
        )
    return mods


def _safe_name(name: str) -> str:
    cleaned = re.sub(r"[^\w.\-]+", "_", name or "mod").strip("._")
    return cleaned or "mod"


@app.get("/api/versions")
async def versions():
    try:
        return {"versions": await list_game_versions()}
    except Exception:
        return {
            "versions": [
                "1.21.10",
                "1.21.8",
                "1.21.5",
                "1.21.4",
                "1.21.3",
                "1.21.1",
                "1.21",
                "1.20.6",
                "1.20.4",
                "1.20.1",
                "1.19.4",
                "1.19.2",
                "1.18.2",
                "1.16.5",
                "1.12.2",
                "1.8.9",
                "1.7.10",
            ]
        }


@app.get("/api/search")
async def search(
    q: str = Query(""),
    source: str = Query("all"),
    type: str = Query("mod"),
    loader: str = Query(""),
    version: str = Query(""),
    sort: str = Query("relevance"),
    offset: int = Query(0, ge=0),
    limit: int = Query(20, ge=1, le=50),
):
    source = source.lower()
    warnings: list[str] = []
    db.log_search(
        q,
        {"source": source, "type": type, "loader": loader, "version": version, "sort": sort},
    )
    if source == "local":
        hits, total = db.search(
            query=q,
            loader=loader,
            version=version,
            project_type=type,
            sort=sort,
            offset=offset,
            limit=limit,
        )
        return {"hits": hits, "total": total, "warnings": warnings}

    tasks = []
    labels = []
    if source in ("all", "modrinth"):
        tasks.append(search_modrinth(q, type, loader, version, sort, offset, limit))
        labels.append("modrinth")
    if source in ("all", "curseforge"):
        key = curseforge_key()
        if key:
            tasks.append(search_curseforge(key, q, type, loader, version, sort, offset, limit))
            labels.append("curseforge")
        else:
            warnings.append("CurseForge is skipped until you add an API key in Settings.")

    groups: list[list[dict]] = []
    totals: list[int] = []
    results = await asyncio.gather(*tasks, return_exceptions=True)
    for label, result in zip(labels, results):
        if isinstance(result, Exception):
            warnings.append(f"{label}: {result}")
            continue
        hits, total = result
        groups.append(hits)
        totals.append(total)

    if source == "all":
        merged = interleave(*groups)
        stored = db.upsert_mods(merged)
        return {"hits": stored, "total": sum(totals), "warnings": warnings}

    hits = groups[0] if groups else []
    total = totals[0] if totals else 0
    stored = db.upsert_mods(hits)
    return {"hits": stored, "total": total, "warnings": warnings}


@app.get("/api/library")
def library(
    q: str = Query(""),
    source: str = Query(""),
    type: str = Query(""),
    loader: str = Query(""),
    version: str = Query(""),
    sort: str = Query("updated"),
    offset: int = Query(0, ge=0),
    limit: int = Query(20, ge=1, le=50),
):
    hits, total = db.search(
        query=q,
        source=source,
        loader=loader,
        version=version,
        project_type=type,
        sort=sort,
        offset=offset,
        limit=limit,
    )
    return {"hits": hits, "total": total, "warnings": []}


@app.get("/api/favorites")
def favorites(
    q: str = Query(""),
    sort: str = Query("updated"),
    offset: int = Query(0, ge=0),
    limit: int = Query(20, ge=1, le=50),
):
    hits, total = db.search(
        query=q,
        favorites_only=True,
        sort=sort,
        offset=offset,
        limit=limit,
    )
    return {"hits": hits, "total": total, "warnings": []}


@app.get("/api/mods/{source}/{source_id}")
async def mod_detail(source: str, source_id: str):
    source = source.lower()
    cached = db.get_mod(source, source_id)
    try:
        if source == "modrinth":
            detail = await get_modrinth_project(source_id)
        elif source == "curseforge":
            key = curseforge_key()
            if not key:
                if cached:
                    return cached
                raise HTTPException(400, "CurseForge API key required")
            detail = await get_curseforge_mod(key, source_id)
        else:
            raise HTTPException(404, "Unknown source")
        stored = db.upsert_mods([detail])[0]
        stored["body"] = detail.get("body") or ""
        stored["files"] = detail.get("files") or []
        stored["source_url"] = detail.get("source_url")
        stored["issues_url"] = detail.get("issues_url")
        stored["wiki_url"] = detail.get("wiki_url")
        stored["discord_url"] = detail.get("discord_url")
        return stored
    except HTTPException:
        raise
    except Exception as exc:
        if cached:
            return cached
        raise HTTPException(502, str(exc)) from exc


@app.get("/api/loader-versions")
async def loader_versions(loader: str = Query(""), version: str = Query("")):
    if not loader:
        return {"versions": []}
    return {"versions": await list_loader_versions(loader, version)}


@app.post("/api/pack/check")
async def pack_check(payload: PackPayload):
    return await check_pack(
        _pack_mods(payload),
        payload.game_version.strip(),
        payload.loader.strip().lower(),
        payload.loader_version.strip(),
        curseforge_key(),
    )


@app.post("/api/pack/download")
async def pack_download(payload: PackPayload):
    report = await check_pack(
        _pack_mods(payload),
        payload.game_version.strip(),
        payload.loader.strip().lower(),
        payload.loader_version.strip(),
        curseforge_key(),
    )
    if not report["resolved"]:
        raise HTTPException(400, "No matching files to download. Check compatibility first.")
    buffer = io.BytesIO()
    failed: list[str] = []
    key = curseforge_key()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as zf:
        for file in report["resolved"]:
            url = file.get("url")
            if not url:
                failed.append(file.get("name") or "unknown")
                continue
            try:
                data = await download_bytes(url, key if file.get("source") == "curseforge" else "")
                filename = _safe_name(file.get("file_name") or f"{file.get('name')}.jar")
                zf.writestr(filename, data)
            except Exception:
                failed.append(file.get("name") or "unknown")
        zf.writestr(
            "compatibility.json",
            json.dumps({**report, "failed": failed}, indent=2, default=str),
        )
    label = "-".join(x for x in [report.get("game_version"), report.get("loader")] if x) or "mods"
    filename = f"moddb-{_safe_name(label)}.zip"
    return Response(
        content=buffer.getvalue(),
        media_type="application/zip",
        headers={"Content-Disposition": f'attachment; filename="{filename}"'},
    )


@app.post("/api/mods/{source}/{source_id}/favorite")
def favorite_mod(source: str, source_id: str, payload: FavoritePayload):
    result = db.set_favorite(source.lower(), source_id, payload.favorite)
    if result is None:
        raise HTTPException(404, "Mod is not in the local database yet. Search it first.")
    return result


app.mount("/static", StaticFiles(directory=STATIC), name="static")


@app.get("/")
def index():
    return FileResponse(STATIC / "index.html")


def main() -> None:
    import uvicorn

    uvicorn.run("server:app", host="127.0.0.1", port=8080, reload=False)


if __name__ == "__main__":
    main()
