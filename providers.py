from __future__ import annotations

import json
import re
from typing import Any
from xml.etree import ElementTree

import httpx

MODRINTH_API = "https://api.modrinth.com/v2"
CURSEFORGE_API = "https://api.curseforge.com/v1"
USER_AGENT = "MCModDatabase/1.0 (local; Minecraft mod search)"

LOADERS = {"fabric", "forge", "neoforge", "quilt", "liteloader", "rift", "babric", "nilloader"}
MR_TYPE_PATH = {
    "mod": "mod",
    "modpack": "modpack",
    "resourcepack": "resourcepack",
    "shader": "shader",
    "datapack": "datapack",
    "plugin": "plugin",
}
CF_CLASS_IDS = {
    "mod": 6,
    "modpack": 4471,
    "resourcepack": 12,
    "shader": 6552,
}
CF_CLASS_TYPES = {v: k for k, v in CF_CLASS_IDS.items()}
CF_LOADER = {
    "forge": 1,
    "fabric": 4,
    "quilt": 5,
    "neoforge": 6,
}
CF_LOADER_NAME = {v: k for k, v in CF_LOADER.items()}
CF_SORT = {
    "relevance": 1,
    "follows": 2,
    "updated": 3,
    "name": 4,
    "downloads": 6,
    "newest": 11,
}
MR_SORT = {
    "relevance": "relevance",
    "downloads": "downloads",
    "follows": "follows",
    "newest": "newest",
    "updated": "updated",
    "name": "relevance",
}


def _client() -> httpx.AsyncClient:
    return httpx.AsyncClient(timeout=20.0, headers={"User-Agent": USER_AGENT})


def _split_mr_categories(categories: list[str] | None) -> tuple[list[str], list[str]]:
    cats = []
    loaders = []
    for item in categories or []:
        key = item.lower()
        if key in LOADERS:
            loaders.append(key)
        else:
            cats.append(item)
    return cats, loaders


def normalize_modrinth(hit: dict[str, Any]) -> dict[str, Any]:
    cats, loaders = _split_mr_categories(hit.get("categories") or hit.get("display_categories"))
    project_type = hit.get("project_type") or "mod"
    slug = hit.get("slug") or hit.get("project_id")
    path = MR_TYPE_PATH.get(project_type, "mod")
    return {
        "source": "modrinth",
        "source_id": hit.get("project_id") or hit.get("id"),
        "slug": slug,
        "name": hit.get("title") or hit.get("name") or "",
        "author": hit.get("author") or "",
        "summary": hit.get("description") or "",
        "icon_url": hit.get("icon_url"),
        "website_url": f"https://modrinth.com/{path}/{slug}",
        "categories": [project_type, *cats],
        "loaders": loaders,
        "game_versions": hit.get("versions") or hit.get("game_versions") or [],
        "downloads": hit.get("downloads") or 0,
        "follows": hit.get("followers") or hit.get("follows") or 0,
        "date_created": hit.get("date_created") or hit.get("published"),
        "date_updated": hit.get("date_modified") or hit.get("updated"),
        "extra": {
            "project_type": project_type,
            "license": hit.get("license") if isinstance(hit.get("license"), str) else (hit.get("license") or {}).get("id"),
            "color": hit.get("color"),
        },
    }


def _cf_loaders(mod: dict[str, Any]) -> list[str]:
    found: list[str] = []
    for index in mod.get("latestFilesIndexes") or []:
        loader = index.get("modLoader")
        name = CF_LOADER_NAME.get(loader)
        if name and name not in found:
            found.append(name)
    for file in mod.get("latestFiles") or []:
        for version in file.get("gameVersions") or []:
            key = str(version).lower()
            if key in LOADERS and key not in found:
                found.append(key)
    return found


def _cf_versions(mod: dict[str, Any]) -> list[str]:
    versions: list[str] = []
    seen: set[str] = set()
    for index in mod.get("latestFilesIndexes") or []:
        ver = index.get("gameVersion")
        if ver and ver not in seen:
            seen.add(ver)
            versions.append(ver)
    return versions


def normalize_curseforge(mod: dict[str, Any]) -> dict[str, Any]:
    authors = mod.get("authors") or []
    author = authors[0]["name"] if authors else ""
    logo = mod.get("logo") or {}
    links = mod.get("links") or {}
    class_id = mod.get("classId")
    project_type = CF_CLASS_TYPES.get(class_id, "mod")
    slug = mod.get("slug") or str(mod.get("id"))
    path = {
        "mod": "mc-mods",
        "modpack": "modpacks",
        "resourcepack": "texture-packs",
        "shader": "shaders",
    }.get(project_type, "mc-mods")
    website = links.get("websiteUrl") or f"https://www.curseforge.com/minecraft/{path}/{slug}"
    categories = [c.get("slug") or c.get("name") for c in (mod.get("categories") or []) if c]
    return {
        "source": "curseforge",
        "source_id": str(mod.get("id")),
        "slug": slug,
        "name": mod.get("name") or "",
        "author": author,
        "summary": mod.get("summary") or "",
        "icon_url": logo.get("thumbnailUrl") or logo.get("url"),
        "website_url": website,
        "categories": [project_type, *[c for c in categories if c]],
        "loaders": _cf_loaders(mod),
        "game_versions": _cf_versions(mod),
        "downloads": int(mod.get("downloadCount") or 0),
        "follows": 0,
        "date_created": mod.get("dateCreated"),
        "date_updated": mod.get("dateModified") or mod.get("dateReleased"),
        "extra": {
            "project_type": project_type,
            "class_id": class_id,
        },
    }


async def search_modrinth(
    query: str,
    project_type: str = "mod",
    loader: str = "",
    version: str = "",
    sort: str = "relevance",
    offset: int = 0,
    limit: int = 20,
) -> tuple[list[dict], int]:
    facets: list[list[str]] = []
    if project_type:
        facets.append([f"project_type:{project_type}"])
    if loader:
        facets.append([f"categories:{loader}"])
    if version:
        facets.append([f"versions:{version}"])
    params: dict[str, Any] = {
        "query": query,
        "index": MR_SORT.get(sort, "relevance"),
        "offset": offset,
        "limit": min(limit, 100),
    }
    if facets:
        params["facets"] = json.dumps(facets)
    async with _client() as client:
        response = await client.get(f"{MODRINTH_API}/search", params=params)
        response.raise_for_status()
        payload = response.json()
    hits = [normalize_modrinth(hit) for hit in payload.get("hits") or []]
    return hits, int(payload.get("total_hits") or 0)


async def search_curseforge(
    api_key: str,
    query: str,
    project_type: str = "mod",
    loader: str = "",
    version: str = "",
    sort: str = "relevance",
    offset: int = 0,
    limit: int = 20,
) -> tuple[list[dict], int]:
    params: dict[str, Any] = {
        "gameId": 432,
        "classId": CF_CLASS_IDS.get(project_type, 6),
        "searchFilter": query,
        "sortField": CF_SORT.get(sort, 1),
        "sortOrder": "asc" if sort == "name" else "desc",
        "pageSize": min(limit, 50),
        "index": offset,
    }
    if version:
        params["gameVersion"] = version
    if loader and loader in CF_LOADER:
        params["modLoaderType"] = CF_LOADER[loader]
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    async with _client() as client:
        response = await client.get(f"{CURSEFORGE_API}/mods/search", params=params, headers=headers)
        response.raise_for_status()
        payload = response.json()
    data = payload.get("data") or []
    pagination = payload.get("pagination") or {}
    hits = [normalize_curseforge(mod) for mod in data]
    return hits, int(pagination.get("totalCount") or len(hits))


async def get_modrinth_project(project_id: str) -> dict[str, Any]:
    async with _client() as client:
        project_res = await client.get(f"{MODRINTH_API}/project/{project_id}")
        project_res.raise_for_status()
        project = project_res.json()
        versions_res = await client.get(f"{MODRINTH_API}/project/{project_id}/version")
        versions_res.raise_for_status()
        versions = versions_res.json()
    normalized = normalize_modrinth({**project, "project_id": project.get("id"), "author": project.get("author") or ""})
    files = []
    for version in versions[:12]:
        files.append(
            {
                "name": version.get("name") or version.get("version_number"),
                "version": version.get("version_number"),
                "loaders": version.get("loaders") or [],
                "game_versions": version.get("game_versions") or [],
                "downloads": version.get("downloads") or 0,
                "date": version.get("date_published"),
                "url": (version.get("files") or [{}])[0].get("url"),
            }
        )
    return {
        **normalized,
        "body": project.get("body") or "",
        "source_url": project.get("source_url"),
        "issues_url": project.get("issues_url"),
        "wiki_url": project.get("wiki_url"),
        "discord_url": project.get("discord_url"),
        "files": files,
    }


async def get_curseforge_mod(api_key: str, mod_id: str) -> dict[str, Any]:
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    async with _client() as client:
        mod_res = await client.get(f"{CURSEFORGE_API}/mods/{mod_id}", headers=headers)
        mod_res.raise_for_status()
        files_res = await client.get(
            f"{CURSEFORGE_API}/mods/{mod_id}/files",
            params={"pageSize": 12},
            headers=headers,
        )
        files_res.raise_for_status()
    mod = (mod_res.json() or {}).get("data") or {}
    files_payload = (files_res.json() or {}).get("data") or []
    normalized = normalize_curseforge(mod)
    files = []
    for file in files_payload[:12]:
        game_versions = file.get("gameVersions") or []
        loaders = [v.lower() for v in game_versions if str(v).lower() in LOADERS]
        versions = [v for v in game_versions if str(v).lower() not in LOADERS]
        files.append(
            {
                "name": file.get("displayName") or file.get("fileName"),
                "version": file.get("displayName"),
                "loaders": loaders,
                "game_versions": versions,
                "downloads": file.get("downloadCount") or 0,
                "date": file.get("fileDate"),
                "url": file.get("downloadUrl"),
            }
        )
    return {
        **normalized,
        "body": "",
        "files": files,
    }


async def list_game_versions() -> list[str]:
    async with _client() as client:
        response = await client.get(f"{MODRINTH_API}/tag/game_version")
        response.raise_for_status()
        payload = response.json()
    versions = [
        item["version"]
        for item in payload
        if item.get("version_type") == "release" and item.get("version")
    ]
    return versions


async def test_curseforge_key(api_key: str) -> bool:
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    async with _client() as client:
        response = await client.get(f"{CURSEFORGE_API}/games/432", headers=headers)
        return response.status_code == 200


CF_DEP_TYPE = {
    1: "embedded",
    2: "optional",
    3: "required",
    4: "optional",
    5: "incompatible",
    6: "embedded",
}


def _mr_file(version: dict[str, Any], project_id: str = "") -> dict[str, Any]:
    files = version.get("files") or [{}]
    primary = next((f for f in files if f.get("primary")), files[0] if files else {})
    deps = []
    for dep in version.get("dependencies") or []:
        deps.append(
            {
                "project_id": str(dep.get("project_id") or ""),
                "version_id": dep.get("version_id"),
                "type": dep.get("dependency_type") or "required",
            }
        )
    return {
        "source": "modrinth",
        "source_id": project_id or version.get("project_id"),
        "file_id": version.get("id"),
        "file_name": primary.get("filename") or version.get("name"),
        "name": version.get("name") or version.get("version_number"),
        "version": version.get("version_number"),
        "loaders": [str(x).lower() for x in (version.get("loaders") or [])],
        "game_versions": version.get("game_versions") or [],
        "downloads": version.get("downloads") or 0,
        "date": version.get("date_published"),
        "url": primary.get("url"),
        "size": primary.get("size") or 0,
        "dependencies": deps,
    }


def _cf_file(file: dict[str, Any], mod_id: str = "") -> dict[str, Any]:
    game_versions = file.get("gameVersions") or []
    loaders = [str(v).lower() for v in game_versions if str(v).lower() in LOADERS]
    versions = [v for v in game_versions if str(v).lower() not in LOADERS]
    deps = []
    for dep in file.get("dependencies") or []:
        deps.append(
            {
                "project_id": str(dep.get("modId") or ""),
                "type": CF_DEP_TYPE.get(dep.get("relationType"), "optional"),
            }
        )
    return {
        "source": "curseforge",
        "source_id": str(mod_id or file.get("modId") or ""),
        "file_id": file.get("id"),
        "file_name": file.get("fileName") or file.get("displayName"),
        "name": file.get("displayName") or file.get("fileName"),
        "version": file.get("displayName"),
        "loaders": loaders,
        "game_versions": versions,
        "downloads": file.get("downloadCount") or 0,
        "date": file.get("fileDate"),
        "url": file.get("downloadUrl"),
        "size": file.get("fileLength") or 0,
        "dependencies": deps,
    }


def _score_file(file: dict[str, Any], loader_version: str) -> int:
    if not loader_version:
        return 0
    blob = " ".join(
        [
            str(file.get("file_name") or ""),
            str(file.get("name") or ""),
            str(file.get("version") or ""),
            " ".join(file.get("game_versions") or []),
        ]
    ).lower()
    return 10 if loader_version.lower() in blob else 0


def _pick_file(files: list[dict[str, Any]], loader_version: str) -> dict[str, Any] | None:
    if not files:
        return None
    return max(files, key=lambda f: (_score_file(f, loader_version), f.get("downloads") or 0))


async def list_loader_versions(loader: str, game_version: str = "") -> list[str]:
    loader = (loader or "").lower()
    versions: list[str] = []
    try:
        async with _client() as client:
            if loader == "fabric":
                url = (
                    f"https://meta.fabricmc.net/v2/versions/loader/{game_version}"
                    if game_version
                    else "https://meta.fabricmc.net/v2/versions/loader"
                )
                payload = (await client.get(url)).json()
                for item in payload:
                    if isinstance(item, dict) and "loader" in item:
                        ver = (item.get("loader") or {}).get("version")
                    else:
                        ver = item.get("version") if isinstance(item, dict) else None
                    if ver:
                        versions.append(str(ver))
            elif loader == "quilt":
                url = "https://meta.quiltmc.org/v3/versions/loader"
                payload = (await client.get(url)).json()
                for item in payload:
                    ver = item.get("version") if isinstance(item, dict) else None
                    if ver:
                        versions.append(str(ver))
            elif loader == "forge":
                promo = (await client.get(
                    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json"
                )).json()
                for key, ver in (promo.get("promos") or {}).items():
                    if game_version and not str(key).startswith(game_version):
                        continue
                    if ver and str(ver) not in versions:
                        versions.append(str(ver))
                meta = await client.get(
                    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml"
                )
                if meta.status_code == 200 and game_version:
                    found = re.findall(
                        rf"<version>{re.escape(game_version)}-([^<]+)</version>",
                        meta.text,
                    )
                    for ver in found:
                        if ver not in versions:
                            versions.append(ver)
            elif loader == "neoforge":
                meta = await client.get(
                    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml"
                )
                if meta.status_code == 200:
                    root = ElementTree.fromstring(meta.text)
                    prefix = ""
                    if game_version:
                        parts = game_version.split(".")
                        if len(parts) >= 3:
                            prefix = f"{parts[1]}.{parts[2]}"
                        elif len(parts) == 2:
                            prefix = f"{parts[1]}.0"
                    for node in root.findall(".//version"):
                        ver = (node.text or "").strip()
                        if ver and (not prefix or ver.startswith(prefix)):
                            versions.append(ver)
    except Exception:
        return versions[:80]
    if loader in {"forge", "neoforge"}:
        versions = list(dict.fromkeys(reversed(versions)))
    else:
        versions = list(dict.fromkeys(versions))
    return versions[:80]


async def resolve_modrinth_file(
    project_id: str,
    game_version: str = "",
    loader: str = "",
    loader_version: str = "",
) -> dict[str, Any] | None:
    params: dict[str, Any] = {}
    if game_version:
        params["game_versions"] = json.dumps([game_version])
    if loader:
        params["loaders"] = json.dumps([loader])
    async with _client() as client:
        response = await client.get(f"{MODRINTH_API}/project/{project_id}/version", params=params)
        response.raise_for_status()
        versions = response.json() or []
    files = [_mr_file(item, project_id) for item in versions]
    return _pick_file(files, loader_version)


async def resolve_curseforge_file(
    api_key: str,
    mod_id: str,
    game_version: str = "",
    loader: str = "",
    loader_version: str = "",
) -> dict[str, Any] | None:
    params: dict[str, Any] = {"pageSize": 50}
    if game_version:
        params["gameVersion"] = game_version
    if loader and loader in CF_LOADER:
        params["modLoaderType"] = CF_LOADER[loader]
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    async with _client() as client:
        response = await client.get(
            f"{CURSEFORGE_API}/mods/{mod_id}/files",
            params=params,
            headers=headers,
        )
        response.raise_for_status()
        payload = response.json()
    files = [_cf_file(item, mod_id) for item in (payload.get("data") or [])]
    picked = _pick_file(files, loader_version)
    if picked and not picked.get("url"):
        picked["url"] = await curseforge_download_url(api_key, mod_id, picked.get("file_id"))
    return picked


async def curseforge_download_url(api_key: str, mod_id: str, file_id: Any) -> str | None:
    if not file_id:
        return None
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    async with _client() as client:
        response = await client.get(
            f"{CURSEFORGE_API}/mods/{mod_id}/files/{file_id}/download-url",
            headers=headers,
        )
        if response.status_code != 200:
            return None
        return (response.json() or {}).get("data")


async def names_modrinth(ids: list[str]) -> dict[str, dict[str, str]]:
    ids = [i for i in dict.fromkeys(ids) if i]
    if not ids:
        return {}
    async with _client() as client:
        response = await client.get(f"{MODRINTH_API}/projects", params={"ids": json.dumps(ids)})
        if response.status_code != 200:
            return {}
        payload = response.json() or []
    out = {}
    for project in payload:
        pid = str(project.get("id") or "")
        out[pid] = {
            "name": project.get("title") or pid,
            "slug": project.get("slug") or pid,
        }
    return out


async def names_curseforge(api_key: str, ids: list[str]) -> dict[str, dict[str, str]]:
    ids = [i for i in dict.fromkeys(ids) if i]
    if not ids or not api_key:
        return {}
    headers = {"Accept": "application/json", "x-api-key": api_key, "User-Agent": USER_AGENT}
    body = {"modIds": [int(i) for i in ids if str(i).isdigit()], "filterPcOnly": True}
    if not body["modIds"]:
        return {}
    async with _client() as client:
        response = await client.post(f"{CURSEFORGE_API}/mods", headers=headers, json=body)
        if response.status_code != 200:
            return {}
        payload = (response.json() or {}).get("data") or []
    out = {}
    for mod in payload:
        mid = str(mod.get("id") or "")
        out[mid] = {
            "name": mod.get("name") or mid,
            "slug": mod.get("slug") or mid,
        }
    return out


async def download_bytes(url: str, api_key: str = "") -> bytes:
    headers = {"User-Agent": USER_AGENT}
    if api_key:
        headers["x-api-key"] = api_key
    async with httpx.AsyncClient(timeout=60.0, follow_redirects=True, headers=headers) as client:
        response = await client.get(url)
        response.raise_for_status()
        return response.content
