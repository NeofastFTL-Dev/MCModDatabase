use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::api::ModHit;
use crate::cache::Library;
use crate::catalog;
use crate::settings::Settings;

const DEFAULT_PORT: u16 = 8765;

pub struct ServerHandle {
    stop: Arc<AtomicBool>,
    port: u16,
}

impl ServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn is_alive(&self) -> bool {
        !self.stop.load(Ordering::SeqCst)
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", self.port).parse().unwrap(),
            Duration::from_millis(200),
        );
    }
}

pub fn generate_api_key() -> String {
    let mut buf = [0u8; 24];
    fill_random(&mut buf);
    let mut s = String::with_capacity(48);
    for b in buf {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn fill_random(buf: &mut [u8]) {
    #[cfg(windows)]
    {
        #[link(name = "advapi32")]
        extern "system" {
            fn SystemFunction036(buf: *mut u8, len: u32) -> u8;
        }
        let ok = unsafe { SystemFunction036(buf.as_mut_ptr(), buf.len() as u32) };
        if ok != 0 {
            return;
        }
    }
    #[cfg(unix)]
    {
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            if f.read_exact(buf).is_ok() {
                return;
            }
        }
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    std::process::id().hash(&mut h);
    let mut n = h.finish();
    for b in buf.iter_mut() {
        n = n.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(1);
        *b = (n >> 24) as u8;
    }
}

pub fn ensure_api_key(settings: &mut Settings) -> String {
    if settings.agent_api_key.trim().is_empty() {
        settings.agent_api_key = generate_api_key();
        settings.save();
    }
    settings.agent_api_key.clone()
}

pub fn default_port(settings: &Settings) -> u16 {
    if settings.agent_port == 0 {
        DEFAULT_PORT
    } else {
        settings.agent_port
    }
}

pub fn run(args: &[String]) -> Result<()> {
    if args.is_empty() {
        print_help();
        return Ok(());
    }
    match args[0].as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "status" => cmd_status(args),
        "search" => cmd_search(args),
        "get" => cmd_get(args),
        "library" => cmd_library(args),
        "key" => cmd_key(args),
        "serve" => cmd_serve(args),
        other => Err(anyhow!("unknown agent command: {other}")),
    }
}

fn want_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json" || a == "-j")
}

fn flag_val<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args.get(i + 1).map(|s| s.as_str());
        }
        if let Some(rest) = args[i].strip_prefix(&format!("{name}=")) {
            return Some(rest);
        }
        i += 1;
    }
    None
}

fn positional_after(args: &[String], skip_flags: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if skip_flags && a.starts_with('-') {
            if matches!(a.as_str(), "--limit" | "--source" | "--loader" | "--version" | "--port" | "--type") {
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    out
}

fn print_help() {
    println!(
        "ModDB agent — query the local catalog for AI tools

Usage:
  ModDB agent status [--json]
  ModDB agent search <query> [--json] [--limit N] [--source all|modrinth|curseforge] [--loader fabric] [--version 1.21.1]
  ModDB agent get <source> <id-or-slug> [--json]
  ModDB agent library [--json]
  ModDB agent key show|generate|clear
  ModDB agent serve [--port 8765]

HTTP (localhost only, Bearer token required except /health):
  GET /health
  GET /status
  GET /search?q=sodium&limit=20
  GET /library
  GET /mods/{{source}}/{{id}}

See AGENTS.md or the in-app Agents tab for connection examples.
"
    );
}

fn emit(json_mode: bool, value: Value, human: &str) {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()));
    } else {
        println!("{human}");
    }
}

fn hit_json(m: &ModHit) -> Value {
    json!({
        "source": m.source,
        "source_id": m.source_id,
        "slug": m.slug,
        "name": m.name,
        "author": m.author,
        "summary": m.summary,
        "icon_url": m.icon_url,
        "website_url": m.website_url,
        "categories": m.categories,
        "loaders": m.loaders,
        "game_versions": m.game_versions,
        "downloads": m.downloads,
    })
}

fn status_value() -> Value {
    let st = catalog::stats();
    let settings = Settings::load();
    let lib = Library::load();
    let (lib_total, _, _, fav) = lib.stats();
    json!({
        "ok": true,
        "app": "MCDB",
        "version": env!("CARGO_PKG_VERSION"),
        "catalog": {
            "path": catalog::path().display().to_string(),
            "total": st.total,
            "modrinth": st.modrinth,
            "curseforge": st.curseforge,
            "last_built": st.last_built,
        },
        "library": { "total": lib_total, "favorites": fav },
        "agent": {
            "port": default_port(&settings),
            "api_key_set": !settings.agent_api_key.trim().is_empty(),
            "base_url": format!("http://127.0.0.1:{}", default_port(&settings)),
        }
    })
}

fn cmd_status(args: &[String]) -> Result<()> {
    let v = status_value();
    let human = format!(
        "MCDB {} — catalog {} mods (MR {} / CF {}), library {}, key {}",
        env!("CARGO_PKG_VERSION"),
        v["catalog"]["total"],
        v["catalog"]["modrinth"],
        v["catalog"]["curseforge"],
        v["library"]["total"],
        if v["agent"]["api_key_set"].as_bool().unwrap_or(false) { "set" } else { "missing" }
    );
    emit(want_json(args), v, &human);
    Ok(())
}

fn cmd_search(args: &[String]) -> Result<()> {
    let rest = positional_after(&args[1..], true);
    let q = rest.first().map(|s| s.as_str()).unwrap_or("").trim();
    if q.is_empty() {
        return Err(anyhow!("usage: ModDB agent search <query> [--json]"));
    }
    let limit: u32 = flag_val(args, "--limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let source = flag_val(args, "--source").unwrap_or("all");
    let loader = flag_val(args, "--loader").unwrap_or("");
    let version = flag_val(args, "--version").unwrap_or("");
    let ptype = flag_val(args, "--type").unwrap_or("mod");
    let hits = catalog::fts_search(q, source, loader, version, ptype, limit);
    let v = json!({
        "ok": true,
        "query": q,
        "count": hits.len(),
        "results": hits.iter().map(hit_json).collect::<Vec<_>>(),
    });
    let human = if hits.is_empty() {
        format!("No results for '{q}'")
    } else {
        hits.iter()
            .map(|m| format!("{}  {}  {}  ({})", m.source, m.name, m.slug, m.downloads))
            .collect::<Vec<_>>()
            .join("\n")
    };
    emit(want_json(args), v, &human);
    Ok(())
}

fn cmd_get(args: &[String]) -> Result<()> {
    let rest = positional_after(&args[1..], true);
    if rest.len() < 2 {
        return Err(anyhow!("usage: ModDB agent get <source> <id-or-slug> [--json]"));
    }
    let source = rest[0].as_str();
    let id = rest[1].as_str();
    match catalog::get_mod(source, id) {
        Some(m) => {
            let v = json!({ "ok": true, "mod": hit_json(&m) });
            let human = format!(
                "{}:{}\n{}\n{}\n{}\n{}",
                m.source, m.source_id, m.name, m.author, m.summary, m.website_url
            );
            emit(want_json(args), v, &human);
            Ok(())
        }
        None => {
            let v = json!({ "ok": false, "error": "not_found", "source": source, "id": id });
            if want_json(args) {
                println!("{}", serde_json::to_string_pretty(&v)?);
                std::process::exit(1);
            }
            Err(anyhow!("mod not found: {source}/{id}"))
        }
    }
}

fn cmd_library(args: &[String]) -> Result<()> {
    let lib = Library::load();
    let items = lib.list_all();
    let v = json!({
        "ok": true,
        "count": items.len(),
        "mods": items.iter().map(|(m, fav)| {
            let mut j = hit_json(m);
            if let Some(obj) = j.as_object_mut() {
                obj.insert("favorite".into(), json!(*fav));
            }
            j
        }).collect::<Vec<_>>(),
    });
    let human = if items.is_empty() {
        "Library is empty".into()
    } else {
        items
            .iter()
            .map(|(m, fav)| {
                format!(
                    "{}{}  {}  {}",
                    if *fav { "*" } else { " " },
                    m.source,
                    m.name,
                    m.slug
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    emit(want_json(args), v, &human);
    Ok(())
}

fn cmd_key(args: &[String]) -> Result<()> {
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("show");
    let mut settings = Settings::load();
    match sub {
        "show" => {
            if settings.agent_api_key.is_empty() {
                println!("(no API key — run: ModDB agent key generate)");
            } else {
                println!("{}", settings.agent_api_key);
            }
            Ok(())
        }
        "generate" | "regen" | "regenerate" => {
            settings.agent_api_key = generate_api_key();
            settings.save();
            println!("{}", settings.agent_api_key);
            eprintln!("API key saved. Use Authorization: Bearer <key> on the local API.");
            Ok(())
        }
        "clear" => {
            settings.agent_api_key.clear();
            settings.save();
            println!("API key cleared");
            Ok(())
        }
        other => Err(anyhow!("usage: ModDB agent key show|generate|clear (got {other})")),
    }
}

fn cmd_serve(args: &[String]) -> Result<()> {
    let mut settings = Settings::load();
    let port = flag_val(args, "--port")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| default_port(&settings));
    settings.agent_port = port;
    let key = ensure_api_key(&mut settings);
    settings.save();
    eprintln!("MCDB agent API on http://127.0.0.1:{port}");
    eprintln!("Authorization: Bearer {key}");
    eprintln!("Ctrl+C to stop. See AGENTS.md / Agents tab for client setup.");
    serve_blocking(port, key, None)
}

pub fn start_background(port: u16, api_key: String) -> Result<ServerHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let key = api_key;
    std::thread::Builder::new()
        .name("mcdb-agent-api".into())
        .spawn(move || {
            if let Err(e) = serve_blocking(port, key, Some(stop_t)) {
                crate::util::log_error(&format!("agent API: {e}"));
            }
        })?;
    // brief wait so bind errors surface via log; assume ok if we got here
    std::thread::sleep(Duration::from_millis(80));
    Ok(ServerHandle { stop, port })
}

pub fn serve_blocking(port: u16, api_key: String, stop: Option<Arc<AtomicBool>>) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| anyhow!("bind 127.0.0.1:{port}: {e}"))?;
    listener.set_nonblocking(true)?;
    loop {
        if stop.as_ref().is_some_and(|s| s.load(Ordering::SeqCst)) {
            break;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                if let Err(e) = handle_client(&mut stream, &api_key) {
                    crate::util::log_error(&format!("agent request: {e}"));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => return Err(anyhow!("accept: {e}")),
        }
    }
    Ok(())
}

fn handle_client(stream: &mut TcpStream, api_key: &str) -> Result<()> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let raw = String::from_utf8_lossy(&buf[..n]);
    let mut lines = raw.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let mut auth = String::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("Authorization") {
                auth = v.trim().to_string();
            }
        }
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };

    if method != "GET" && method != "HEAD" {
        return write_json(stream, 405, &json!({"ok": false, "error": "method_not_allowed"}));
    }

    let needs_auth = path != "/health";
    if needs_auth && !auth_ok(&auth, api_key) {
        return write_json(stream, 401, &json!({"ok": false, "error": "unauthorized"}));
    }

    match path {
        "/health" => write_json(stream, 200, &json!({"ok": true, "service": "mcdb-agent", "version": env!("CARGO_PKG_VERSION")})),
        "/status" => write_json(stream, 200, &status_value()),
        "/search" => {
            let q = query_param(query, "q").unwrap_or_default();
            if q.is_empty() {
                return write_json(stream, 400, &json!({"ok": false, "error": "missing_q"}));
            }
            let limit: u32 = query_param(query, "limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(20)
                .clamp(1, 100);
            let source = query_param(query, "source").unwrap_or_else(|| "all".into());
            let loader = query_param(query, "loader").unwrap_or_default();
            let version = query_param(query, "version").unwrap_or_default();
            let ptype = query_param(query, "type").unwrap_or_else(|| "mod".into());
            let hits = catalog::fts_search(&q, &source, &loader, &version, &ptype, limit);
            write_json(
                stream,
                200,
                &json!({
                    "ok": true,
                    "query": q,
                    "count": hits.len(),
                    "results": hits.iter().map(hit_json).collect::<Vec<_>>(),
                }),
            )
        }
        "/library" => {
            let lib = Library::load();
            let items = lib.list_all();
            write_json(
                stream,
                200,
                &json!({
                    "ok": true,
                    "count": items.len(),
                    "mods": items.iter().map(|(m, fav)| {
                        let mut j = hit_json(m);
                        if let Some(obj) = j.as_object_mut() {
                            obj.insert("favorite".into(), json!(*fav));
                        }
                        j
                    }).collect::<Vec<_>>(),
                }),
            )
        }
        p if p.starts_with("/mods/") => {
            let rest = p.trim_start_matches("/mods/");
            let mut segs = rest.splitn(2, '/');
            let source = segs.next().unwrap_or("");
            let id = segs.next().unwrap_or("");
            if source.is_empty() || id.is_empty() {
                return write_json(stream, 400, &json!({"ok": false, "error": "bad_path"}));
            }
            let id = percent_decode(id);
            match catalog::get_mod(source, &id) {
                Some(m) => write_json(stream, 200, &json!({"ok": true, "mod": hit_json(&m)})),
                None => write_json(stream, 404, &json!({"ok": false, "error": "not_found"})),
            }
        }
        _ => write_json(stream, 404, &json!({"ok": false, "error": "not_found"})),
    }
}

fn auth_ok(header: &str, api_key: &str) -> bool {
    if api_key.is_empty() {
        return false;
    }
    let h = header.trim();
    if let Some(rest) = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")) {
        return rest.trim() == api_key;
    }
    // also accept raw key for simple tools
    h == api_key
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Some(percent_decode(v));
            }
        } else if pair == name {
            return Some(String::new());
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn write_json(stream: &mut TcpStream, status: u16, body: &Value) -> Result<()> {
    let payload = serde_json::to_vec_pretty(body)?;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        payload.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&payload)?;
    Ok(())
}
