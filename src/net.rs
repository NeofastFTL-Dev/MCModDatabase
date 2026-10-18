use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "ModDB/",
    env!("CARGO_PKG_VERSION"),
    " (personal desktop app; CurseForge + Modrinth search)"
);

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let mut b = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(60));
        if let Ok(tls) = native_tls::TlsConnector::new() {
            b = b.tls_connector(Arc::new(tls));
        }
        b.build()
    })
}

pub fn describe_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => format!("HTTP {code} {}", r.status_text()),
        ureq::Error::Transport(t) => format!("network: {t}"),
    }
}

pub fn get_json(url: &str, extra: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut req = agent().get(url);
    for (k, v) in extra {
        req = req.set(k, v);
    }
    req.call()
        .map_err(|e| anyhow!("{}", describe_err(e)))?
        .into_json()
        .context("json")
}

pub fn download_to<W: Write>(
    url: &str,
    extra: &[(&str, &str)],
    writer: &mut W,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    let mut req = agent().get(url);
    for (k, v) in extra {
        req = req.set(k, v);
    }
    let resp = req.call().map_err(|e| anyhow!("{}", describe_err(e)))?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.trim().parse::<u64>().ok());
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        let n = reader.read(&mut buf).context("read")?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    Ok(done)
}

pub fn get_bytes_limited(url: &str, max: usize) -> Result<Vec<u8>> {
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| anyhow!("{}", describe_err(e)))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::new();
    loop {
        let n = reader.read(&mut buf).context("read")?;
        if n == 0 {
            break;
        }
        if out.len().saturating_add(n) > max {
            return Err(anyhow!("too large"));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

pub fn get_text(url: &str) -> Result<String> {
    let mut v = Vec::new();
    download_to(url, &[], &mut v, |_, _| {})?;
    Ok(match String::from_utf8(v) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(&e.into_bytes()).into_owned(),
    })
}

pub fn download_file(
    url: &str,
    extra: &[(&str, &str)],
    dest: &Path,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    let res = (|| -> Result<u64> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&part)?);
        let n = download_to(url, extra, &mut f, progress)?;
        f.flush()?;
        drop(f);
        std::fs::rename(&part, dest)?;
        Ok(n)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    res
}
