//! A task's long-form note, read just far enough to lay it out: headings,
//! paragraphs, bullets, checkboxes and code. Not a Markdown implementation.

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Bullet {
        depth: u8,
        /// `Some` for a `- [ ]` or `- [x]` item.
        check: Option<bool>,
        text: String,
    },
    Code(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Span {
    pub text: String,
    pub code: bool,
    pub bold: bool,
}

fn bullet(line: &str) -> Option<(u8, Option<bool>, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = line.trim_start();
    let body = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0)
                .then(|| rest[digits..].strip_prefix(". "))
                .flatten()
        })?;
    let (check, text) = if let Some(t) = body.strip_prefix("[ ] ") {
        (Some(false), t)
    } else if let Some(t) = body
        .strip_prefix("[x] ")
        .or_else(|| body.strip_prefix("[X] "))
    {
        (Some(true), t)
    } else {
        (None, body)
    };
    Some(((indent / 2).min(3) as u8, check, text))
}

pub fn parse(md: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut code: Option<String> = None;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(c) => out.push(Block::Code(c.trim_end().to_string())),
                None => code = Some(String::new()),
            }
            continue;
        }
        if let Some(c) = code.as_mut() {
            c.push_str(line);
            c.push('\n');
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            // A blank line ends a paragraph: mark it so the next line starts
            // a new one.
            if matches!(out.last(), Some(Block::Para(p)) if !p.is_empty()) {
                out.push(Block::Para(String::new()));
            }
            continue;
        }
        let hashes = t.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            drop_empty(&mut out);
            out.push(Block::Heading(hashes as u8, t[hashes..].trim().to_string()));
        } else if let Some((depth, check, text)) = bullet(line) {
            drop_empty(&mut out);
            out.push(Block::Bullet {
                depth,
                check,
                text: text.trim().to_string(),
            });
        } else {
            // A wrapped line continues the paragraph or bullet above it.
            match out.last_mut() {
                Some(Block::Para(p)) => {
                    if !p.is_empty() {
                        p.push(' ');
                    }
                    p.push_str(t);
                }
                Some(Block::Bullet { text, .. }) if line.starts_with(' ') => {
                    text.push(' ');
                    text.push_str(t);
                }
                _ => out.push(Block::Para(t.to_string())),
            }
        }
    }
    if let Some(c) = code {
        out.push(Block::Code(c.trim_end().to_string()));
    }
    drop_empty(&mut out);
    out
}

fn drop_empty(out: &mut Vec<Block>) {
    if matches!(out.last(), Some(Block::Para(p)) if p.is_empty()) {
        out.pop();
    }
}

/// The choices a decision offers: the bullets under an "Options" heading.
pub fn options(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for block in parse(md) {
        match block {
            Block::Heading(_, t) => inside = t.trim().eq_ignore_ascii_case("options"),
            Block::Bullet { text, depth: 0, .. } if inside => out.push(text),
            _ => {}
        }
    }
    out
}

/// Splits a line at `code` and **bold** marks.
pub fn spans(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let (mut code, mut bold) = (false, false);
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut flush = |cur: &mut String, code: bool, bold: bool| {
        if !cur.is_empty() {
            out.push(Span {
                text: std::mem::take(cur),
                code,
                bold,
            });
        }
    };
    while let Some(c) = chars.next() {
        if c == '`' {
            flush(&mut cur, code, bold);
            code = !code;
        } else if c == '*' && !code && chars.peek() == Some(&'*') {
            chars.next();
            flush(&mut cur, code, bold);
            bold = !bold;
        } else {
            cur.push(c);
        }
    }
    flush(&mut cur, code, bold);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_a_note() {
        let blocks = parse(
            "# Playtest the slice\n\nBuilt 2026-11-09.\nWhen a run runs dry, the card opens.\n\n\
             Things to judge:\n- Does it feel earned?\n  The capacitor costs 6.\n- [x] Is it readable?\n  - [ ] nested\n\n\
             ## Keys\n```\nW ends the run\n```\n1. first\n",
        );
        assert_eq!(blocks[0], Block::Heading(1, "Playtest the slice".into()));
        assert_eq!(
            blocks[1],
            Block::Para("Built 2026-11-09. When a run runs dry, the card opens.".into())
        );
        assert_eq!(blocks[2], Block::Para("Things to judge:".into()));
        assert_eq!(
            blocks[3],
            Block::Bullet {
                depth: 0,
                check: None,
                text: "Does it feel earned? The capacitor costs 6.".into()
            }
        );
        assert!(matches!(
            &blocks[4],
            Block::Bullet {
                check: Some(true),
                ..
            }
        ));
        assert!(matches!(
            &blocks[5],
            Block::Bullet {
                depth: 1,
                check: Some(false),
                ..
            }
        ));
        assert_eq!(blocks[6], Block::Heading(2, "Keys".into()));
        assert_eq!(blocks[7], Block::Code("W ends the run".into()));
        assert!(matches!(&blocks[8], Block::Bullet { text, .. } if text == "first"));
        assert_eq!(blocks.len(), 9);
    }

    #[test]
    fn a_decision_lists_its_options() {
        let md = "# Pick the rule\n\nContext.\n\n## Options\n\n- Keep 4 s\n- Lower it\n  to 2 s\n\n## Decided\n\n- Keep 4 s\n";
        assert_eq!(options(md), ["Keep 4 s", "Lower it to 2 s"]);
        assert!(options("# No choices here\n- a bullet\n").is_empty());
    }

    #[test]
    fn marks() {
        let s = spans("lower `run.start_charge` in **data**");
        assert_eq!(s.len(), 4);
        assert!(s[1].code && s[1].text == "run.start_charge");
        assert!(s[3].bold && s[3].text == "data");
    }
}
