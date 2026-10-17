from __future__ import annotations

import asyncio
from collections import defaultdict
from typing import Any

from providers import (
    names_curseforge,
    names_modrinth,
    resolve_curseforge_file,
    resolve_modrinth_file,
)


def _key(mod: dict) -> str:
    return f"{mod.get('source')}:{mod.get('source_id')}"


def _intersect(sets: list[set[str]]) -> set[str]:
    if not sets:
        return set()
    out = sets[0]
    for item in sets[1:]:
        out = out & item
    return out


def _issue(severity: str, kind: str, message: str, **extra) -> dict:
    return {"severity": severity, "type": kind, "message": message, **extra}


async def check_pack(
    mods: list[dict],
    game_version: str = "",
    loader: str = "",
    loader_version: str = "",
    curseforge_key: str = "",
) -> dict[str, Any]:
    issues: list[dict] = []
    if len(mods) < 1:
        return {
            "ok": True,
            "game_version": game_version,
            "loader": loader,
            "loader_version": loader_version,
            "issues": [_issue("info", "empty", "Select mods to check compatibility.")],
            "resolved": [],
        }

    version_sets = [set(m.get("game_versions") or []) for m in mods if m.get("game_versions")]
    loader_sets = [set(x.lower() for x in (m.get("loaders") or [])) for m in mods if m.get("loaders")]
    common_versions = _intersect(version_sets) if version_sets else set()
    common_loaders = _intersect(loader_sets) if loader_sets else set()

    if not game_version:
        if len(common_versions) == 1:
            game_version = next(iter(common_versions))
        elif not common_versions:
            issues.append(
                _issue(
                    "error",
                    "mc_version",
                    "These mods share no Minecraft version. Pick one in the sidebar, or remove the mismatch.",
                )
            )
        else:
            sample = ", ".join(sorted(common_versions, reverse=True)[:8])
            issues.append(
                _issue(
                    "warn",
                    "mc_version",
                    f"Pick a Minecraft version. Shared versions include: {sample}",
                    versions=sorted(common_versions, reverse=True)[:12],
                )
            )
    else:
        missing_mc = [
            m["name"]
            for m in mods
            if m.get("game_versions") and game_version not in m["game_versions"]
        ]
        if missing_mc:
            issues.append(
                _issue(
                    "error",
                    "mc_version",
                    f"No listed {game_version} support: {', '.join(missing_mc[:8])}",
                    mods=missing_mc,
                )
            )

    if not loader:
        if len(common_loaders) == 1:
            loader = next(iter(common_loaders))
        elif not common_loaders:
            issues.append(
                _issue(
                    "error",
                    "loader",
                    "These mods share no loader (Fabric / Forge / NeoForge / Quilt).",
                )
            )
        else:
            issues.append(
                _issue(
                    "warn",
                    "loader",
                    f"Pick a loader. Shared loaders: {', '.join(sorted(common_loaders))}",
                    loaders=sorted(common_loaders),
                )
            )
    else:
        missing_loader = [
            m["name"]
            for m in mods
            if m.get("loaders") and loader.lower() not in [x.lower() for x in m["loaders"]]
        ]
        if missing_loader:
            issues.append(
                _issue(
                    "error",
                    "loader",
                    f"Not listed for {loader}: {', '.join(missing_loader[:8])}",
                    mods=missing_loader,
                )
            )

    by_slug: dict[str, list[dict]] = defaultdict(list)
    by_name: dict[str, list[dict]] = defaultdict(list)
    for mod in mods:
        slug = (mod.get("slug") or "").lower()
        name = (mod.get("name") or "").strip().lower()
        if slug:
            by_slug[slug].append(mod)
        if name:
            by_name[name].append(mod)
    for group in list(by_slug.values()) + list(by_name.values()):
        sources = {(m.get("source"), str(m.get("source_id"))) for m in group}
        if len(sources) > 1:
            labels = [f"{m['name']} ({m['source']})" for m in group]
            issues.append(
                _issue(
                    "warn",
                    "duplicate",
                    f"Same mod selected twice: {', '.join(labels)}",
                    mods=[m["name"] for m in group],
                )
            )

    selected_ids = {
        "modrinth": {str(m["source_id"]) for m in mods if m.get("source") == "modrinth"},
        "curseforge": {str(m["source_id"]) for m in mods if m.get("source") == "curseforge"},
    }
    selected_slugs = {(m.get("slug") or "").lower() for m in mods if m.get("slug")}

    resolved: list[dict] = []
    sem = asyncio.Semaphore(8)

    async def resolve_one(mod: dict) -> dict:
        async with sem:
            source = mod.get("source")
            source_id = str(mod.get("source_id"))
            file = None
            error = None
            try:
                if source == "modrinth":
                    file = await resolve_modrinth_file(source_id, game_version, loader, loader_version)
                elif source == "curseforge":
                    if not curseforge_key:
                        error = "CurseForge API key required for this mod"
                    else:
                        file = await resolve_curseforge_file(
                            curseforge_key, source_id, game_version, loader, loader_version
                        )
            except Exception as exc:
                error = str(exc)
            return {"mod": mod, "file": file, "error": error}

    results = await asyncio.gather(*[resolve_one(m) for m in mods])

    dep_ids = {"modrinth": [], "curseforge": []}
    for item in results:
        file = item.get("file") or {}
        source = (item["mod"].get("source") or "")
        for dep in file.get("dependencies") or []:
            if dep.get("project_id"):
                dep_ids.setdefault(source, []).append(str(dep["project_id"]))

    names = {
        "modrinth": await names_modrinth(dep_ids.get("modrinth") or []),
        "curseforge": await names_curseforge(curseforge_key, dep_ids.get("curseforge") or []),
    }

    for item in results:
        mod = item["mod"]
        file = item["file"]
        if item["error"]:
            issues.append(_issue("error", "resolve", f"{mod['name']}: {item['error']}", mod=mod["name"]))
            continue
        if not file:
            target = " ".join(x for x in [loader, game_version] if x) or "the selected version"
            issues.append(
                _issue(
                    "error",
                    "no_file",
                    f"{mod['name']} has no file for {target}",
                    mod=mod["name"],
                )
            )
            continue
        for dep in file.get("dependencies") or []:
            dep_id = str(dep.get("project_id") or "")
            info = names.get(mod["source"], {}).get(dep_id) or {}
            dep_name = info.get("name") or dep_id
            dep_slug = (info.get("slug") or "").lower()
            present = dep_id in selected_ids.get(mod["source"], set()) or (
                dep_slug and dep_slug in selected_slugs
            )
            kind = dep.get("type")
            if kind == "incompatible" and present:
                issues.append(
                    _issue(
                        "error",
                        "incompatible",
                        f"{mod['name']} is marked incompatible with {dep_name}",
                        mod=mod["name"],
                        other=dep_name,
                    )
                )
            elif kind == "required" and not present:
                issues.append(
                    _issue(
                        "warn",
                        "missing_dep",
                        f"{mod['name']} requires {dep_name}, which is not in the selection",
                        mod=mod["name"],
                        other=dep_name,
                    )
                )
        resolved.append(
            {
                **file,
                "name": mod.get("name"),
                "slug": mod.get("slug"),
                "icon_url": mod.get("icon_url"),
                "website_url": mod.get("website_url"),
            }
        )

    errors = [i for i in issues if i["severity"] == "error"]
    unique = []
    seen = set()
    for issue in issues:
        marker = (issue["type"], issue["message"])
        if marker in seen:
            continue
        seen.add(marker)
        unique.append(issue)
    return {
        "ok": not errors,
        "game_version": game_version,
        "loader": loader,
        "loader_version": loader_version,
        "issues": unique,
        "resolved": resolved,
    }
