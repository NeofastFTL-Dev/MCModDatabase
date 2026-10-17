use eframe::egui::{self, Color32, RichText};

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
    t.starts_with('<')
        || t.contains("<p")
        || t.contains("<div")
        || t.contains("<br")
        || t.contains("<h1")
        || t.contains("<h2")
        || t.contains("<img")
        || t.contains("<span")
        || t.contains("<ul")
        || t.contains("<a ")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let (ch, n) = if rest.starts_with("&nbsp;") {
            ('\u{00a0}', 6)
        } else if rest.starts_with("&amp;") {
            ('&', 5)
        } else if rest.starts_with("&lt;") {
            ('<', 4)
        } else if rest.starts_with("&gt;") {
            ('>', 4)
        } else if rest.starts_with("&quot;") {
            ('"', 6)
        } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
            ('\'', if rest.starts_with("&#39;") { 5 } else { 6 })
        } else if rest.starts_with("&#x") || rest.starts_with("&#X") {
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
            out.push('&');
            rest = &rest[1..];
            continue;
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
            out.push('&');
            rest = &rest[1..];
            continue;
        } else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        out.push(ch);
        rest = &rest[n..];
    }
    out.push_str(rest);
    out.replace('\u{00a0}', " ")
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
        if last.style.bold == st.bold
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
                let alt = attr(raw, "alt").unwrap_or_default();
                let src = attr(raw, "src").unwrap_or_default();
                let label = if alt.is_empty() { "[image]".into() } else { alt };
                if src.starts_with("http") {
                    buf.push(Inline {
                        text: label,
                        style: Style {
                            italic: true,
                            link: Some(src),
                            ..st.clone()
                        },
                    });
                } else {
                    push_text(&mut buf, &format!(" {label} "), &Style { italic: true, ..st.clone() });
                }
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
    Some((text, url, i + 1))
}
