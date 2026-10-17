from __future__ import annotations

import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from threading import Lock

ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
DB_PATH = DATA_DIR / "mods.db"


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _json(value) -> str:
    return json.dumps(value, ensure_ascii=False)


def _loads(value, default):
    if not value:
        return default
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return default


class Database:
    def __init__(self, path: Path = DB_PATH) -> None:
        DATA_DIR.mkdir(parents=True, exist_ok=True)
        self.path = path
        self._lock = Lock()
        self.conn = sqlite3.connect(path, check_same_thread=False)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("PRAGMA foreign_keys=ON")
        self._init()

    def _init(self) -> None:
        self.conn.executescript(
            """
            CREATE TABLE IF NOT EXISTS mods (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                source_id TEXT NOT NULL,
                slug TEXT,
                name TEXT NOT NULL,
                author TEXT,
                summary TEXT,
                icon_url TEXT,
                website_url TEXT,
                categories TEXT,
                loaders TEXT,
                game_versions TEXT,
                downloads INTEGER DEFAULT 0,
                follows INTEGER DEFAULT 0,
                date_created TEXT,
                date_updated TEXT,
                last_fetched TEXT,
                extra TEXT,
                UNIQUE(source, source_id)
            );
            CREATE TABLE IF NOT EXISTS favorites (
                mod_id INTEGER PRIMARY KEY REFERENCES mods(id) ON DELETE CASCADE,
                added_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS search_history (
                id INTEGER PRIMARY KEY,
                query TEXT NOT NULL,
                filters TEXT,
                searched_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mods_name ON mods(name);
            CREATE INDEX IF NOT EXISTS idx_mods_author ON mods(author);
            CREATE INDEX IF NOT EXISTS idx_mods_downloads ON mods(downloads);
            CREATE INDEX IF NOT EXISTS idx_mods_updated ON mods(date_updated);
            CREATE INDEX IF NOT EXISTS idx_mods_source ON mods(source);
            """
        )
        self.conn.commit()

    def close(self) -> None:
        self.conn.close()

    def _row(self, row: sqlite3.Row | None, favorite_ids: set[int] | None = None) -> dict | None:
        if row is None:
            return None
        data = dict(row)
        data["categories"] = _loads(data.get("categories"), [])
        data["loaders"] = _loads(data.get("loaders"), [])
        data["game_versions"] = _loads(data.get("game_versions"), [])
        extra = _loads(data.pop("extra", None), {})
        if extra:
            data["extra"] = extra
        data["favorite"] = bool(data.get("favorite")) if "favorite" in data else (
            row["id"] in favorite_ids if favorite_ids is not None else False
        )
        return data

    def upsert_mods(self, mods: list[dict]) -> list[dict]:
        if not mods:
            return []
        now = _now()
        with self._lock:
            for mod in mods:
                self.conn.execute(
                    """
                    INSERT INTO mods (
                        source, source_id, slug, name, author, summary, icon_url,
                        website_url, categories, loaders, game_versions, downloads,
                        follows, date_created, date_updated, last_fetched, extra
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(source, source_id) DO UPDATE SET
                        slug=excluded.slug,
                        name=excluded.name,
                        author=excluded.author,
                        summary=excluded.summary,
                        icon_url=excluded.icon_url,
                        website_url=excluded.website_url,
                        categories=excluded.categories,
                        loaders=excluded.loaders,
                        game_versions=excluded.game_versions,
                        downloads=excluded.downloads,
                        follows=excluded.follows,
                        date_created=excluded.date_created,
                        date_updated=excluded.date_updated,
                        last_fetched=excluded.last_fetched,
                        extra=excluded.extra
                    """,
                    (
                        mod["source"],
                        str(mod["source_id"]),
                        mod.get("slug"),
                        mod["name"],
                        mod.get("author"),
                        mod.get("summary"),
                        mod.get("icon_url"),
                        mod.get("website_url"),
                        _json(mod.get("categories") or []),
                        _json(mod.get("loaders") or []),
                        _json(mod.get("game_versions") or []),
                        int(mod.get("downloads") or 0),
                        int(mod.get("follows") or 0),
                        mod.get("date_created"),
                        mod.get("date_updated"),
                        now,
                        _json(mod.get("extra") or {}),
                    ),
                )
            self.conn.commit()
            ids = [(m["source"], str(m["source_id"])) for m in mods]
            placeholders = ",".join("(?, ?)" for _ in ids)
            flat = [item for pair in ids for item in pair]
            rows = self.conn.execute(
                f"""
                SELECT m.*, CASE WHEN f.mod_id IS NULL THEN 0 ELSE 1 END AS favorite
                FROM mods m
                LEFT JOIN favorites f ON f.mod_id = m.id
                WHERE (m.source, m.source_id) IN ({placeholders})
                """,
                flat,
            ).fetchall()
        by_key = {(r["source"], r["source_id"]): self._row(r) for r in rows}
        return [by_key.get((m["source"], str(m["source_id"])), m) for m in mods]

    def get_mod(self, source: str, source_id: str) -> dict | None:
        row = self.conn.execute(
            """
            SELECT m.*, CASE WHEN f.mod_id IS NULL THEN 0 ELSE 1 END AS favorite
            FROM mods m
            LEFT JOIN favorites f ON f.mod_id = m.id
            WHERE m.source = ? AND m.source_id = ?
            """,
            (source, str(source_id)),
        ).fetchone()
        return self._row(row)

    def search(
        self,
        query: str = "",
        source: str = "",
        loader: str = "",
        version: str = "",
        project_type: str = "",
        favorites_only: bool = False,
        sort: str = "updated",
        offset: int = 0,
        limit: int = 20,
    ) -> tuple[list[dict], int]:
        clauses = []
        params: list = []
        if query.strip():
            like = f"%{query.strip()}%"
            clauses.append("(m.name LIKE ? OR m.author LIKE ? OR m.summary LIKE ? OR m.slug LIKE ?)")
            params.extend([like, like, like, like])
        if source and source not in ("all", "local"):
            clauses.append("m.source = ?")
            params.append(source)
        if loader:
            clauses.append("lower(m.loaders) LIKE ?")
            params.append(f"%\"{loader.lower()}\"%")
        if version:
            clauses.append("m.game_versions LIKE ?")
            params.append(f"%\"{version}\"%")
        if project_type:
            clauses.append("lower(coalesce(json_extract(m.extra, '$.project_type'), '')) = ?")
            params.append(project_type.lower())
        if favorites_only:
            clauses.append("f.mod_id IS NOT NULL")
        where = f"WHERE {' AND '.join(clauses)}" if clauses else ""
        join = "LEFT JOIN favorites f ON f.mod_id = m.id"
        if favorites_only:
            join = "INNER JOIN favorites f ON f.mod_id = m.id"
        order = {
            "downloads": "m.downloads DESC",
            "updated": "m.date_updated DESC",
            "name": "m.name COLLATE NOCASE ASC",
            "follows": "m.follows DESC",
            "newest": "m.date_created DESC",
            "relevance": "m.downloads DESC",
        }.get(sort, "m.last_fetched DESC")
        count_sql = f"SELECT COUNT(*) FROM mods m {join} {where}"
        list_sql = f"""
            SELECT m.*, CASE WHEN f.mod_id IS NULL THEN 0 ELSE 1 END AS favorite
            FROM mods m
            {join}
            {where}
            ORDER BY {order}
            LIMIT ? OFFSET ?
        """
        with self._lock:
            total = self.conn.execute(count_sql, params).fetchone()[0]
            rows = self.conn.execute(list_sql, [*params, limit, offset]).fetchall()
        return [self._row(r) for r in rows], total

    def set_favorite(self, source: str, source_id: str, favorite: bool) -> dict | None:
        with self._lock:
            row = self.conn.execute(
                "SELECT id FROM mods WHERE source = ? AND source_id = ?",
                (source, str(source_id)),
            ).fetchone()
            if row is None:
                return None
            if favorite:
                self.conn.execute(
                    "INSERT OR REPLACE INTO favorites (mod_id, added_at) VALUES (?, ?)",
                    (row["id"], _now()),
                )
            else:
                self.conn.execute("DELETE FROM favorites WHERE mod_id = ?", (row["id"],))
            self.conn.commit()
        return self.get_mod(source, str(source_id))

    def log_search(self, query: str, filters: dict) -> None:
        with self._lock:
            self.conn.execute(
                "INSERT INTO search_history (query, filters, searched_at) VALUES (?, ?, ?)",
                (query, _json(filters), _now()),
            )
            self.conn.execute(
                """
                DELETE FROM search_history WHERE id NOT IN (
                    SELECT id FROM search_history ORDER BY searched_at DESC LIMIT 100
                )
                """
            )
            self.conn.commit()

    def recent_searches(self, limit: int = 8) -> list[str]:
        rows = self.conn.execute(
            """
            SELECT DISTINCT query FROM search_history
            WHERE query != ''
            ORDER BY searched_at DESC
            LIMIT ?
            """,
            (limit,),
        ).fetchall()
        return [r["query"] for r in rows]

    def stats(self) -> dict:
        row = self.conn.execute(
            """
            SELECT
                COUNT(*) AS total,
                SUM(CASE WHEN source = 'modrinth' THEN 1 ELSE 0 END) AS modrinth,
                SUM(CASE WHEN source = 'curseforge' THEN 1 ELSE 0 END) AS curseforge,
                (SELECT COUNT(*) FROM favorites) AS favorites
            FROM mods
            """
        ).fetchone()
        return {
            "total": row["total"] or 0,
            "modrinth": row["modrinth"] or 0,
            "curseforge": row["curseforge"] or 0,
            "favorites": row["favorites"] or 0,
        }
