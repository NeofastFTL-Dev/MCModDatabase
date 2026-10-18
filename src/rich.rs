use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const DIM: Color32 = Color32::from_rgb(141, 151, 168);
const CODE_BG: Color32 = Color32::from_rgb(23, 28, 38);

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
    heading: u8,
    link: Option<String>,
}

struct Inline {
    text: String,
    style: Style,
    image: Option<String>,
}

enum Fetch {
    Loading,
    Done,
    Fail,
}

pub fn render(ui: &mut egui::Ui, src: &str) {
    let src = if src.len() > 80_000 {
        let mut end = 80_000;
        while end > 0 && !src.is_char_boundary(end) {
            end -= 1;
        }
        &src[..end]
    } else {
        src
    };
    if looks_like_html(src) {
        render_html(ui, src);
    } else {
        render_markdown(ui, src);
    }
}

fn looks_like_html(s: &str) -> bool {
    let t = s.trim_start();
    let l = t.to_ascii_lowercase();
    if l.starts_with("<!doctype") || l.starts_with("<html") || l.starts_with("<p") || l.starts_with("<div") {
        return true;
    }
    let markers = [
        "<p>", "<p ", "<div", "<br", "<ul", "<ol", "<li", "<h1", "<h2", "<table", "<span", "<center",
    ];
    markers.iter().filter(|m| l.contains(*m)).count() >= 2
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "nbsp" => '\u{00a0}',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "mdash" => '-',
        "ndash" => '-',
        "hellip" => '…',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "rsquo" | "rsquor" => '’',
        "lsquo" => '‘',
        "rdquo" => '”',
        "ldquo" => '“',
        "sbquo" => '‚',
        "bdquo" => '„',
        "bull" => '•',
        "middot" => '·',
        "times" => '×',
        "minus" => '−',
        "plusmn" => '±',
        "raquo" => '»',
        "laquo" => '«',
        "deg" => '°',
        "frac12" => '½',
        "frac14" => '¼',
        "frac34" => '¾',
        "euro" => '€',
        "pound" => '£',
        "yen" => '¥',
        "cent" => '¢',
        "sect" => '§',
        "para" => '¶',
        "dagger" => '†',
        "Dagger" => '‡',
        "permil" => '‰',
        "lsaquo" => '‹',
        "rsaquo" => '›',
        "iexcl" => '¡',
        "iquest" => '¿',
        "szlig" => 'ß',
        "agrave" => 'à',
        "aacute" => 'á',
        "acirc" => 'â',
        "atilde" => 'ã',
        "auml" => 'ä',
        "aring" => 'å',
        "aelig" => 'æ',
        "ccedil" => 'ç',
        "egrave" => 'è',
        "eacute" => 'é',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "igrave" => 'ì',
        "iacute" => 'í',
        "icirc" => 'î',
        "iuml" => 'ï',
        "eth" => 'ð',
        "ntilde" => 'ñ',
        "ograve" => 'ò',
        "oacute" => 'ó',
        "ocirc" => 'ô',
        "otilde" => 'õ',
        "ouml" => 'ö',
        "oslash" => 'ø',
        "ugrave" => 'ù',
        "uacute" => 'ú',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "yacute" => 'ý',
        "yuml" => 'ÿ',
        "thorn" => 'þ',
        "Agrave" => 'À',
        "Aacute" => 'Á',
        "Acirc" => 'Â',
        "Atilde" => 'Ã',
        "Auml" => 'Ä',
        "Aring" => 'Å',
        "AElig" => 'Æ',
        "Ccedil" => 'Ç',
        "Egrave" => 'È',
        "Eacute" => 'É',
        "Ecirc" => 'Ê',
        "Euml" => 'Ë',
        "Igrave" => 'Ì',
        "Iacute" => 'Í',
        "Icirc" => 'Î',
        "Iuml" => 'Ï',
        "Ntilde" => 'Ñ',
        "Ograve" => 'Ò',
        "Oacute" => 'Ó',
        "Ocirc" => 'Ô',
        "Otilde" => 'Õ',
        "Ouml" => 'Ö',
        "Oslash" => 'Ø',
        "Ugrave" => 'Ù',
        "Uacute" => 'Ú',
        "Ucirc" => 'Û',
        "Uuml" => 'Ü',
        "Yacute" => 'Ý',
        "fnof" => 'ƒ',
        "circ" => 'ˆ',
        "tilde" => '˜',
        _ => return None,
    })
}

fn decode_entities_once(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with("&#x") || rest.starts_with("&#X") {
            if let Some(end) = rest.find(';') {
                let hex = &rest[3..end];
                if let Ok(v) = u32::from_str_radix(hex, 16) {
                    if let Some(c) = char::from_u32(v) {
                        out.push(c);
                        rest = &rest[end + 1..];
                        continue;
                    }
                }
            }
        } else if rest.starts_with("&#") {
            if let Some(end) = rest.find(';') {
                let num = &rest[2..end];
                if let Ok(v) = num.parse::<u32>() {
                    if let Some(c) = char::from_u32(v) {
                        out.push(c);
                        rest = &rest[end + 1..];
                        continue;
                    }
                }
            }
        } else if let Some(end) = rest.find(';') {
            if end > 1 && end <= 10 {
                let name = &rest[1..end];
                if let Some(c) = named_entity(name) {
                    out.push(c);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out.replace('\u{00a0}', " ")
}

fn decode_entities(s: &str) -> String {
    let once = decode_entities_once(s);
    if once.contains('&') {
        decode_entities_once(&once)
    } else {
        once
    }
}

fn intern_uri(url: &str) -> &'static str {
    static MAP: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut map = MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(u) = map.get(url) {
        return u;
    }
    let leaked: &'static str = Box::leak(url.to_string().into_boxed_str());
    map.insert(url.to_string(), leaked);
    leaked
}

fn fetch_map() -> &'static Mutex<HashMap<&'static str, Fetch>> {
    static MAP: OnceLock<Mutex<HashMap<&'static str, Fetch>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    let comma = url.find(',')?;
    let (meta, data) = url.split_at(comma);
    let data = data[1..].as_bytes();
    if !meta.to_ascii_lowercase().contains(";base64") {
        return Some(data.to_vec());
    }
    fn val(c: u8) -> Option<u8> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let filtered: Vec<u8> = data
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut i = 0;
    while i + 3 < filtered.len() {
        let a = val(filtered[i])?;
        let b = val(filtered[i + 1])?;
        let c = val(filtered[i + 2])?;
        let d = val(filtered[i + 3])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
        out.push((c << 6) | d);
        i += 4;
    }
    if i + 1 < filtered.len() {
        let a = val(filtered[i])?;
        let b = val(filtered[i + 1])?;
        out.push((a << 2) | (b >> 4));
        if i + 2 < filtered.len() {
            let c = val(filtered[i + 2])?;
            out.push((b << 4) | (c >> 2));
        }
    }
    Some(out)
}

fn normalize_url(src: &str) -> Option<String> {
    let s = decode_entities(src)
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string();
    if s.is_empty() || s.starts_with("javascript:") {
        return None;
    }
    if s.starts_with("data:image/") {
        return Some(s);
    }
    if let Some(rest) = s.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if s.starts_with("https://") || s.starts_with("http://") {
        return Some(s);
    }
    None
}

fn kick_fetch(ctx: &egui::Context, url: &'static str) {
    {
        let mut st = fetch_map().lock().unwrap_or_else(|e| e.into_inner());
        if st.contains_key(url) {
            return;
        }
        st.insert(url, Fetch::Loading);
    }
    if url.starts_with("data:") {
        if let Some(bytes) = decode_data_url(url) {
            ctx.include_bytes(url, bytes);
            fetch_map()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(url, Fetch::Done);
        } else {
            fetch_map()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(url, Fetch::Fail);
        }
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let ok = crate::net::get_bytes_limited(url, 8 * 1024 * 1024)
            .ok()
            .filter(|b| !b.is_empty());
        let mut st = fetch_map().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bytes) = ok {
            ctx.include_bytes(url, bytes);
            st.insert(url, Fetch::Done);
        } else {
            st.insert(url, Fetch::Fail);
        }
        drop(st);
        ctx.request_repaint();
    });
}

fn show_image(ui: &mut egui::Ui, src: &str, alt: &str) {
    let Some(url) = normalize_url(src) else {
        if !alt.is_empty() {
            ui.label(RichText::new(alt).italics().color(DIM));
        }
        return;
    };
    let uri = intern_uri(&url);
    kick_fetch(ui.ctx(), uri);
    let state = fetch_map()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(uri)
        .map(|s| match s {
            Fetch::Loading => 0u8,
            Fetch::Done => 1,
            Fetch::Fail => 2,
        })
        .unwrap_or(0);
    match state {
        1 => {
            let max_w = ui.available_width().min(560.0);
            let resp = ui.add(
                egui::Image::new(uri)
                    .max_width(max_w)
                    .max_height(420.0)
                    .maintain_aspect_ratio(true),
            );
            if !alt.is_empty() {
                resp.on_hover_text(alt);
            }
        }
        2 => {
            let label = if alt.is_empty() { "[image]" } else { alt };
            if url.starts_with("http") {
                ui.hyperlink_to(label, &url);
            } else {
                ui.label(RichText::new(label).italics().color(DIM));
            }
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(if alt.is_empty() {
                    "Loading image…"
                } else {
                    alt
                }).color(DIM));
            });
        }
    }
}

fn apply(text: &str, st: &Style) -> RichText {
    let mut r = RichText::new(text);
    if st.bold || st.heading > 0 {
        r = r.strong();
    }
    if st.italic {
        r = r.italics();
    }
    if st.code {
        r = r.monospace().background_color(CODE_BG);
    }
    match st.heading {
        1 => r = r.size(22.0),
        2 => r = r.size(18.0),
        3 => r = r.size(16.0),
        4..=6 => r = r.size(15.0),
        _ => {}
    }
    r
}

fn flush(ui: &mut egui::Ui, buf: &mut Vec<Inline>) {
    if buf.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for inl in buf.drain(..) {
            if inl.text.is_empty() {
                continue;
            }
            if let Some(url) = &inl.image {
                show_image(ui, url, &inl.text);
                continue;
            }
            for chunk in inl.text.split_inclusive(|c: char| c.is_whitespace()) {
                if chunk.is_empty() {
                    continue;
                }
                if let Some(url) = &inl.style.link {
                    if !url.is_empty() {
                        ui.hyperlink_to(chunk, url);
                        continue;
                    }
                }
                ui.label(apply(chunk, &inl.style));
            }
        }
    });
}

fn push_text(buf: &mut Vec<Inline>, text: &str, st: &Style) {
    let text = decode_entities(text);
    if text.is_empty() {
        return;
    }
    if let Some(last) = buf.last_mut() {
        if last.image.is_none()
            && last.style.bold == st.bold
            && last.style.italic == st.italic
            && last.style.code == st.code
            && last.style.heading == st.heading
            && last.style.link == st.link
        {
            last.text.push_str(&text);
            return;
        }
    }
    buf.push(Inline {
        text,
        style: st.clone(),
        image: None,
    });
}

fn push_image(buf: &mut Vec<Inline>, url: String, alt: String) {
    buf.push(Inline {
        text: alt,
        style: Style::default(),
        image: Some(url),
    });
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let l = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let Some(i) = l.find(&key) else {
        return None;
    };
    let rest = tag[i + key.len()..].trim_start();
    if rest.starts_with('"') {
        let r = &rest[1..];
        r.find('"').map(|e| decode_entities(&r[..e]))
    } else if rest.starts_with('\'') {
        let r = &rest[1..];
        r.find('\'').map(|e| decode_entities(&r[..e]))
    } else {
        let e = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        Some(decode_entities(&rest[..e]))
    }
}

fn tag_name(raw: &str) -> String {
    let t = raw.trim().trim_start_matches('/');
    t.split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn render_html(ui: &mut egui::Ui, html: &str) {
    let mut buf = Vec::new();
    let mut st = Style::default();
    let mut skip = 0u32;
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        if i > 0 {
            if skip == 0 {
                push_text(&mut buf, &rest[..i], &st);
            }
        }
        rest = &rest[i + 1..];
        if rest.starts_with("!--") {
            if let Some(e) = rest.find("-->") {
                rest = &rest[e + 3..];
            } else {
                break;
            }
            continue;
        }
        let Some(e) = rest.find('>') else {
            break;
        };
        let raw = &rest[..e];
        rest = &rest[e + 1..];
        let closing = raw.starts_with('/');
        let name = tag_name(raw);
        let empty = !closing && (raw.ends_with('/') || matches!(name.as_str(), "br" | "img" | "hr" | "meta" | "input" | "link"));
        if skip > 0 {
            if closing && matches!(name.as_str(), "script" | "style" | "iframe" | "object" | "embed" | "noscript" | "svg") {
                skip -= 1;
            } else if !closing && !empty && matches!(name.as_str(), "script" | "style" | "iframe" | "object" | "embed" | "noscript" | "svg") {
                skip += 1;
            }
            continue;
        }
        if !closing && matches!(name.as_str(), "script" | "style" | "iframe" | "object" | "embed" | "noscript" | "svg") {
            if !empty {
                skip += 1;
            }
            continue;
        }
        match (closing, name.as_str()) {
            (false, "br") => {
                flush(ui, &mut buf);
            }
            (false, "hr") => {
                flush(ui, &mut buf);
                ui.separator();
            }
            (false, "img") => {
                flush(ui, &mut buf);
                let alt = attr(raw, "alt").unwrap_or_default();
                let src = attr(raw, "src")
                    .or_else(|| attr(raw, "data-src"))
                    .unwrap_or_default();
                show_image(ui, &src, &alt);
            }
            (false, "p" | "div" | "section" | "article" | "tr" | "blockquote" | "pre" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                flush(ui, &mut buf);
                if name.starts_with('h') {
                    if let Some(d) = name.as_bytes().get(1).and_then(|c| char::from(*c).to_digit(10)) {
                        st.heading = d as u8;
                    }
                }
                if name == "pre" {
                    st.code = true;
                }
                if name == "blockquote" {
                    st.italic = true;
                }
            }
            (true, "p" | "div" | "section" | "article" | "tr" | "blockquote" | "pre" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li") => {
                if name == "li" {
                    if let Some(first) = buf.first_mut() {
                        first.text = format!("• {}", first.text);
                    } else {
                        push_text(&mut buf, "• ", &st);
                    }
                }
                flush(ui, &mut buf);
                if name.starts_with('h') {
                    st.heading = 0;
                    ui.add_space(4.0);
                }
                if name == "pre" {
                    st.code = false;
                }
                if name == "blockquote" {
                    st.italic = false;
                }
                if name == "p" || name == "div" || name == "li" {
                    ui.add_space(2.0);
                }
            }
            (false, "li") => {
                flush(ui, &mut buf);
            }
            (false, "b" | "strong") => st.bold = true,
            (true, "b" | "strong") => st.bold = false,
            (false, "i" | "em") => st.italic = true,
            (true, "i" | "em") => st.italic = false,
            (false, "code" | "tt" | "kbd") => st.code = true,
            (true, "code" | "tt" | "kbd") => st.code = false,
            (false, "a") => st.link = attr(raw, "href"),
            (true, "a") => st.link = None,
            _ => {}
        }
    }
    if skip == 0 && !rest.is_empty() {
        push_text(&mut buf, rest, &st);
    }
    flush(ui, &mut buf);
}

fn render_markdown(ui: &mut egui::Ui, src: &str) {
    let mut fence = false;
    let mut fence_buf = String::new();
    for line in src.lines() {
        if line.trim_start().starts_with("```") {
            if fence {
                ui.add_space(4.0);
                ui.label(RichText::new(&fence_buf).monospace().background_color(CODE_BG));
                fence_buf.clear();
                fence = false;
            } else {
                fence = true;
            }
            continue;
        }
        if fence {
            fence_buf.push_str(line);
            fence_buf.push('\n');
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(6.0);
            continue;
        }
        if trimmed.starts_with('|') && trimmed.contains('|') {
            ui.label(RichText::new(trimmed).small().color(DIM));
            continue;
        }
        let (heading, rest) = if let Some(r) = trimmed.strip_prefix("###### ") {
            (6, r)
        } else if let Some(r) = trimmed.strip_prefix("##### ") {
            (5, r)
        } else if let Some(r) = trimmed.strip_prefix("#### ") {
            (4, r)
        } else if let Some(r) = trimmed.strip_prefix("### ") {
            (3, r)
        } else if let Some(r) = trimmed.strip_prefix("## ") {
            (2, r)
        } else if let Some(r) = trimmed.strip_prefix("# ") {
            (1, r)
        } else {
            (0, trimmed)
        };
        let rest = if let Some(r) = rest.strip_prefix("> ") {
            r
        } else {
            rest
        };
        let (bullet, rest) = if let Some(r) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
            (true, r)
        } else {
            (false, rest)
        };
        let mut buf = Vec::new();
        let mut st = Style {
            heading,
            italic: trimmed.starts_with("> "),
            ..Default::default()
        };
        if bullet {
            push_text(&mut buf, "• ", &st);
        }
        parse_md_inlines(&mut buf, rest, &mut st);
        flush(ui, &mut buf);
        if heading > 0 {
            ui.add_space(4.0);
        }
    }
}

fn parse_md_inlines(buf: &mut Vec<Inline>, s: &str, st: &mut Style) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut acc = String::new();
    let flush_acc = |buf: &mut Vec<Inline>, acc: &mut String, st: &Style| {
        if !acc.is_empty() {
            push_text(buf, acc, st);
            acc.clear();
        }
    };
    while i < chars.len() {
        if chars[i] == '`' {
            flush_acc(buf, &mut acc, st);
            i += 1;
            let mut code = String::new();
            while i < chars.len() && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            let mut cs = st.clone();
            cs.code = true;
            push_text(buf, &code, &cs);
            continue;
        }
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some((text, url, n)) = parse_md_link(&chars[i + 1..]) {
                flush_acc(buf, &mut acc, st);
                push_image(buf, url, text);
                i += 1 + n;
                continue;
            }
        }
        if chars[i] == '<' {
            let rest: String = chars[i..].iter().collect();
            let low = rest.to_ascii_lowercase();
            if low.starts_with("<img") {
                if let Some(end) = rest.find('>') {
                    let tag = &rest[1..end];
                    flush_acc(buf, &mut acc, st);
                    let alt = attr(tag, "alt").unwrap_or_default();
                    let src = attr(tag, "src")
                        .or_else(|| attr(tag, "data-src"))
                        .unwrap_or_default();
                    push_image(buf, src, alt);
                    i += end + 1;
                    continue;
                }
            }
            if low.starts_with("<br") {
                if let Some(end) = rest.find('>') {
                    flush_acc(buf, &mut acc, st);
                    push_text(buf, "\n", st);
                    i += end + 1;
                    continue;
                }
            }
        }
        if chars[i] == '[' {
            if let Some((text, url, n)) = parse_md_link(&chars[i..]) {
                flush_acc(buf, &mut acc, st);
                let mut ls = st.clone();
                ls.link = Some(url);
                push_text(buf, &text, &ls);
                i += n;
                continue;
            }
        }
        if chars[i] == '*' || chars[i] == '_' {
            let mark = chars[i];
            let double = i + 1 < chars.len() && chars[i + 1] == mark;
            flush_acc(buf, &mut acc, st);
            i += if double { 2 } else { 1 };
            if double {
                st.bold = !st.bold;
            } else {
                st.italic = !st.italic;
            }
            continue;
        }
        acc.push(chars[i]);
        i += 1;
    }
    flush_acc(buf, &mut acc, st);
}

fn parse_md_link(chars: &[char]) -> Option<(String, String, usize)> {
    if chars.first() != Some(&'[') {
        return None;
    }
    let mut i = 1;
    let mut text = String::new();
    while i < chars.len() && chars[i] != ']' {
        text.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() || i + 1 >= chars.len() || chars[i] != ']' || chars[i + 1] != '(' {
        return None;
    }
    i += 2;
    let mut url = String::new();
    while i < chars.len() && chars[i] != ')' {
        url.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let url = url
        .split_whitespace()
        .next()
        .unwrap_or(url.as_str())
        .to_string();
    Some((text, url, i + 1))
}
