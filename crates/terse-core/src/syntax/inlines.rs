//! Delimiter-aware inline parsing: emphasis/strong, code, links, footnotes,
//! inline math, narrative/parenthetical citations, and cross-references.
//!
//! Operates on already-flattened paragraph/heading/abstract text (physical
//! line breaks already joined into semantic spaces by the block layer), so
//! code/math spans are guaranteed to stay on one logical line by
//! construction.

const ESCAPABLE: &str = "@[]{}*^\\`()~_%$&#";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Code(String),
    Link { label: Vec<Inline>, destination: String },
    Footnote(Vec<Inline>),
    Math(String),
    CrossRef(String),
    Citation(Citation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Citation {
    Narrative(String),
    Group(Vec<CiteItem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiteItem {
    pub alias: String,
    pub locator: Option<Locator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locator {
    pub kind: Option<LocatorKind>,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatorKind {
    Page,
    Pages,
    Chapter,
    Section,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineError(pub String);

/// Returns the plain-text concatenation of a node's readable content,
/// ignoring cross-references/citations. Used where callers only need a
/// coarse text projection (e.g. generated-document fallbacks).
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain(inlines, &mut out);
    out
}

fn push_plain(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) => out.push_str(t),
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Footnote(v) => push_plain(v, out),
            Inline::Link { label, .. } => push_plain(label, out),
            Inline::CrossRef(_) | Inline::Citation(_) => {}
        }
    }
}

/// Pandoc's rule for `$...$`: the opener at `chars[pos]` (which must be
/// `$`) must be followed by a non-space, and the closer must follow a
/// non-space and must not precede a digit. Only the next unescaped `$` is
/// a candidate closer (TeX forbids a bare `$` inside inline math), so a
/// price before real math never pairs with it. Returns the closer's
/// index, or `None` when the `$` is literal text (a price, a lone sign).
/// A crate-level function (not just a `Parser` method) so the pipe-table
/// cell splitter (`blocks.rs`) can use the exact same rule the inline
/// parser does, and the two can never disagree about where `$...$` ends.
pub(crate) fn find_dollar_closer(chars: &[char], pos: usize) -> Option<usize> {
    match chars.get(pos + 1) {
        Some(c) if !c.is_whitespace() => {}
        _ => return None,
    }
    let mut i = pos + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '$' => {
                let prev = chars[i - 1];
                let next_is_digit = matches!(chars.get(i + 1), Some(c) if c.is_ascii_digit());
                return (!prev.is_whitespace() && !next_is_digit).then_some(i);
            }
            _ => i += 1,
        }
    }
    None
}

pub fn parse_inline(input: &str) -> Result<Vec<Inline>, InlineError> {
    let mut parser = Parser {
        chars: input.chars().collect(),
        pos: 0,
        in_footnote: false,
        in_link_label: false,
    };
    let out = parser.parse_until(&|_: &Parser| false)?;
    if parser.pos != parser.chars.len() {
        return Err(InlineError("unexpected trailing content".to_string()));
    }
    Ok(out)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    in_footnote: bool,
    in_link_label: bool,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn is_name_start(c: char) -> bool {
        c.is_ascii_alphabetic()
    }

    fn is_name_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '-'
    }

    fn starts_with(&self, s: &str) -> bool {
        let cs: Vec<char> = s.chars().collect();
        if self.pos + cs.len() > self.chars.len() {
            return false;
        }
        self.chars[self.pos..self.pos + cs.len()] == cs[..]
    }

    fn flush(buf: &mut String, out: &mut Vec<Inline>) {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    }

    fn token_boundary(prev: Option<char>) -> bool {
        match prev {
            None => true,
            Some(c) => !(c.is_alphanumeric() || c == '_'),
        }
    }

    fn parse_until(&mut self, is_end: &dyn Fn(&Parser) -> bool) -> Result<Vec<Inline>, InlineError> {
        let mut out = Vec::new();
        let mut buf = String::new();

        while self.pos < self.chars.len() && !is_end(self) {
            let c = self.chars[self.pos];
            match c {
                '\\' if self.starts_with("\\(") => {
                    Self::flush(&mut buf, &mut out);
                    self.parse_math(&mut out)?;
                }
                '\\' => {
                    match self.peek_at(self.pos + 1) {
                        Some(esc) if ESCAPABLE.contains(esc) => {
                            buf.push(esc);
                            self.pos += 2;
                        }
                        _ => return Err(InlineError("unknown backslash escape".to_string())),
                    }
                }
                '$' if self.peek_at(self.pos + 1) == Some('$') => {
                    return Err(InlineError(
                        "display math '$$' must start its own line".to_string(),
                    ));
                }
                '$' => match self.find_dollar_closer() {
                    Some(close) => {
                        Self::flush(&mut buf, &mut out);
                        let content: String = self.chars[self.pos + 1..close].iter().collect();
                        out.push(Inline::Math(content));
                        self.pos = close + 1;
                    }
                    None => {
                        buf.push('$');
                        self.pos += 1;
                    }
                },
                '`' => {
                    Self::flush(&mut buf, &mut out);
                    self.parse_code(&mut out)?;
                }
                '^' if self.peek_at(self.pos + 1) == Some('[') => {
                    if self.in_footnote {
                        return Err(InlineError("footnotes cannot nest".to_string()));
                    }
                    Self::flush(&mut buf, &mut out);
                    self.parse_footnote(&mut out)?;
                }
                '{' if self.starts_with("{ref:") => {
                    Self::flush(&mut buf, &mut out);
                    self.parse_cross_ref(&mut out)?;
                }
                '[' if self.peek_at(self.pos + 1) == Some('@') => {
                    Self::flush(&mut buf, &mut out);
                    self.parse_citation_group(&mut out)?;
                }
                '[' if !self.in_link_label => {
                    Self::flush(&mut buf, &mut out);
                    if !self.try_parse_link(&mut out)? {
                        buf.push('[');
                        self.pos += 1;
                    }
                }
                '@' if Self::token_boundary(buf.chars().last()) => {
                    if let Some((alias, new_pos)) = self.try_scan_alias(self.pos + 1) {
                        Self::flush(&mut buf, &mut out);
                        out.push(Inline::Citation(Citation::Narrative(alias)));
                        self.pos = new_pos;
                    } else {
                        buf.push('@');
                        self.pos += 1;
                    }
                }
                '*' if self.starts_with("***") => {
                    return Err(InlineError(
                        "unescaped '***' run; use explicit nesting instead".to_string(),
                    ));
                }
                '*' => {
                    let strong = self.starts_with("**");
                    let marker_len = if strong { 2 } else { 1 };
                    let next = self.peek_at(self.pos + marker_len);
                    let prev_word = buf.chars().last().map(|c| c.is_alphanumeric()).unwrap_or(false);
                    let is_opener = matches!(next, Some(nc) if !nc.is_whitespace()) && !prev_word;
                    if is_opener {
                        Self::flush(&mut buf, &mut out);
                        out.push(self.parse_emphasis(strong)?);
                    } else {
                        buf.push('*');
                        self.pos += 1;
                    }
                }
                other => {
                    buf.push(other);
                    self.pos += 1;
                }
            }
        }

        Self::flush(&mut buf, &mut out);
        Ok(out)
    }

    fn parse_emphasis(&mut self, strong: bool) -> Result<Inline, InlineError> {
        let marker_len = if strong { 2 } else { 1 };
        self.pos += marker_len;
        let closer = move |p: &Parser| p.is_emphasis_closer(strong);
        let inner = self.parse_until(&closer)?;
        if self.pos >= self.chars.len() {
            return Err(InlineError(if strong {
                "unterminated strong span".to_string()
            } else {
                "unterminated emphasis span".to_string()
            }));
        }
        self.pos += marker_len;
        if inner.is_empty() {
            return Err(InlineError("empty emphasis/strong span".to_string()));
        }
        Ok(if strong {
            Inline::Strong(inner)
        } else {
            Inline::Emphasis(inner)
        })
    }

    fn is_emphasis_closer(&self, strong: bool) -> bool {
        if strong {
            if !self.starts_with("**") {
                return false;
            }
        } else if self.peek() != Some('*') {
            return false;
        }
        let prev = if self.pos > 0 { Some(self.chars[self.pos - 1]) } else { None };
        matches!(prev, Some(c) if !c.is_whitespace())
    }

    fn parse_code(&mut self, out: &mut Vec<Inline>) -> Result<(), InlineError> {
        let mut run_len = 0;
        while self.peek_at(self.pos + run_len) == Some('`') {
            run_len += 1;
        }
        self.pos += run_len;
        let content_start = self.pos;
        loop {
            if self.pos >= self.chars.len() {
                return Err(InlineError("unterminated code span".to_string()));
            }
            if self.peek() == Some('`') {
                let mut close_len = 0;
                while self.peek_at(self.pos + close_len) == Some('`') {
                    close_len += 1;
                }
                if close_len == run_len {
                    let content: String = self.chars[content_start..self.pos].iter().collect();
                    self.pos += close_len;
                    out.push(Inline::Code(content));
                    return Ok(());
                }
                self.pos += close_len;
                continue;
            }
            self.pos += 1;
        }
    }

    fn parse_math(&mut self, out: &mut Vec<Inline>) -> Result<(), InlineError> {
        self.pos += 2;
        let content_start = self.pos;
        loop {
            if self.pos >= self.chars.len() {
                return Err(InlineError("unterminated inline math".to_string()));
            }
            if self.starts_with("\\)") {
                let content: String = self.chars[content_start..self.pos].iter().collect();
                self.pos += 2;
                out.push(Inline::Math(content));
                return Ok(());
            }
            self.pos += 1;
        }
    }

    /// Pandoc's rule for `$...$`: the opener at `self.pos` must be followed
    /// by a non-space, and the closer must follow a non-space and must not
    /// precede a digit. Only the next unescaped `$` is a candidate closer
    /// (TeX forbids a bare `$` inside inline math), so a price before real
    /// math never pairs with it. Returns the closer's index, or `None` when
    /// the `$` is literal text (a price, a lone sign).
    fn find_dollar_closer(&self) -> Option<usize> {
        find_dollar_closer(&self.chars, self.pos)
    }

    fn parse_footnote(&mut self, out: &mut Vec<Inline>) -> Result<(), InlineError> {
        self.pos += 2;
        let prev_in_footnote = self.in_footnote;
        self.in_footnote = true;
        let inner = self.parse_until(&|p: &Parser| p.peek() == Some(']'));
        self.in_footnote = prev_in_footnote;
        let inner = inner?;
        if self.peek() != Some(']') {
            return Err(InlineError("unterminated footnote".to_string()));
        }
        self.pos += 1;
        out.push(Inline::Footnote(inner));
        Ok(())
    }

    fn parse_cross_ref(&mut self, out: &mut Vec<Inline>) -> Result<(), InlineError> {
        self.pos += "{ref:".chars().count();
        while self.peek() == Some(' ') {
            self.pos += 1;
        }
        let start = self.pos;
        if !matches!(self.peek(), Some(c) if Self::is_name_start(c)) {
            return Err(InlineError("expected an id after '{ref:'".to_string()));
        }
        self.pos += 1;
        while matches!(self.peek(), Some(c) if Self::is_name_char(c)) {
            self.pos += 1;
        }
        let id: String = self.chars[start..self.pos].iter().collect();
        while self.peek() == Some(' ') {
            self.pos += 1;
        }
        if self.peek() != Some('}') {
            return Err(InlineError("expected '}' to close a cross-reference".to_string()));
        }
        self.pos += 1;
        out.push(Inline::CrossRef(id));
        Ok(())
    }

    fn try_parse_link(&mut self, out: &mut Vec<Inline>) -> Result<bool, InlineError> {
        let save = self.pos;
        self.pos += 1;
        let prev_in_label = self.in_link_label;
        self.in_link_label = true;
        let label_result = self.parse_until(&|p: &Parser| p.starts_with("]("));
        self.in_link_label = prev_in_label;
        let label = match label_result {
            Ok(l) => l,
            Err(_) => {
                self.pos = save;
                return Ok(false);
            }
        };
        if !self.starts_with("](") {
            self.pos = save;
            return Ok(false);
        }
        self.pos += 2;
        let dest_start = self.pos;
        let mut depth = 0i32;
        loop {
            match self.peek() {
                None => {
                    self.pos = save;
                    return Ok(false);
                }
                Some('\\') if matches!(self.peek_at(self.pos + 1), Some('(') | Some(')')) => {
                    self.pos += 2;
                }
                Some('(') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(')') => {
                    if depth == 0 {
                        let raw: String = self.chars[dest_start..self.pos].iter().collect();
                        self.pos += 1;
                        let destination = raw.replace("\\(", "(").replace("\\)", ")");
                        if destination.chars().any(|c| c.is_whitespace()) {
                            return Err(InlineError(
                                "link destination cannot contain unescaped whitespace".to_string(),
                            ));
                        }
                        out.push(Inline::Link { label, destination });
                        return Ok(true);
                    }
                    depth -= 1;
                    self.pos += 1;
                }
                Some(_) => {
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_citation_group(&mut self, out: &mut Vec<Inline>) -> Result<(), InlineError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            if self.peek() != Some('@') {
                return Err(InlineError("expected '@alias' in citation group".to_string()));
            }
            self.pos += 1;
            let (alias, new_pos) = self
                .try_scan_alias(self.pos)
                .ok_or_else(|| InlineError("expected an alias after '@'".to_string()))?;
            self.pos = new_pos;

            let mut locator = None;
            if self.peek() == Some(',') {
                self.pos += 1;
                if self.peek() == Some(' ') {
                    self.pos += 1;
                }
                let mut raw = String::new();
                loop {
                    match self.peek() {
                        None => return Err(InlineError("unterminated citation locator".to_string())),
                        Some(';') | Some(']') => break,
                        Some('\\') => {
                            let esc = self.peek_at(self.pos + 1);
                            match esc {
                                Some(c) if ";]\\".contains(c) => {
                                    raw.push(c);
                                    self.pos += 2;
                                }
                                _ => return Err(InlineError("unknown escape in locator".to_string())),
                            }
                        }
                        Some(c) => {
                            raw.push(c);
                            self.pos += 1;
                        }
                    }
                }
                locator = Some(normalize_locator(raw));
            }
            items.push(CiteItem { alias, locator });

            match self.peek() {
                Some(';') => {
                    self.pos += 1;
                    if self.peek() == Some(' ') {
                        self.pos += 1;
                    }
                }
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(InlineError("expected ';' or ']' in citation group".to_string())),
            }
        }
        out.push(Inline::Citation(Citation::Group(items)));
        Ok(())
    }

    fn try_scan_alias(&self, start: usize) -> Option<(String, usize)> {
        if !matches!(self.peek_at(start), Some(c) if Self::is_name_start(c)) {
            return None;
        }
        let mut end = start + 1;
        while matches!(self.peek_at(end), Some(c) if Self::is_name_char(c)) {
            end += 1;
        }
        let alias: String = self.chars[start..end].iter().collect();
        Some((alias, end))
    }
}

fn normalize_locator(raw: String) -> Locator {
    let trimmed = raw.trim_end().to_string();
    const PREFIXES: &[(&str, LocatorKind)] = &[
        ("pp. ", LocatorKind::Pages),
        ("p. ", LocatorKind::Page),
        ("ch. ", LocatorKind::Chapter),
        ("sec. ", LocatorKind::Section),
        ("vol. ", LocatorKind::Volume),
    ];
    for (prefix, kind) in PREFIXES {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Locator {
                kind: Some(*kind),
                raw: rest.to_string(),
            };
        }
    }
    Locator { kind: None, raw: trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_prose_stays_text() {
        let out = parse_inline("hello world").unwrap();
        assert_eq!(out, vec![Inline::Text("hello world".to_string())]);
    }

    #[test]
    fn test_emphasis_and_strong() {
        let out = parse_inline("*em* and **strong**").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Emphasis(vec![Inline::Text("em".to_string())]),
                Inline::Text(" and ".to_string()),
                Inline::Strong(vec![Inline::Text("strong".to_string())]),
            ]
        );
    }

    #[test]
    fn test_word_internal_asterisks_are_text() {
        let out = parse_inline("co*author*ship").unwrap();
        assert_eq!(out, vec![Inline::Text("co*author*ship".to_string())]);
    }

    #[test]
    fn test_code_and_math_are_literal() {
        let out = parse_inline("`a@b*c` and \\(x^2\\)").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Code("a@b*c".to_string()),
                Inline::Text(" and ".to_string()),
                Inline::Math("x^2".to_string()),
            ]
        );
    }

    #[test]
    fn test_triple_asterisk_rejected() {
        assert!(parse_inline("a ***b*** c").is_err());
    }

    #[test]
    fn test_crossing_delimiters_fail() {
        assert!(parse_inline("*emph [link*text](dest)").is_err());
    }

    #[test]
    fn test_narrative_citation_vs_email() {
        let out = parse_inline("@robins1986 and jane@example.com").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Citation(Citation::Narrative("robins1986".to_string())),
                Inline::Text(" and jane@example.com".to_string()),
            ]
        );
    }

    #[test]
    fn test_citation_group_with_locators() {
        let out = parse_inline("[@robins1986, pp. 10-12; @pearl2009, p. 42]").unwrap();
        assert_eq!(
            out,
            vec![Inline::Citation(Citation::Group(vec![
                CiteItem {
                    alias: "robins1986".to_string(),
                    locator: Some(Locator {
                        kind: Some(LocatorKind::Pages),
                        raw: "10-12".to_string()
                    }),
                },
                CiteItem {
                    alias: "pearl2009".to_string(),
                    locator: Some(Locator {
                        kind: Some(LocatorKind::Page),
                        raw: "42".to_string()
                    }),
                },
            ]))]
        );
    }

    #[test]
    fn test_link_and_cross_reference() {
        let out = parse_inline("[label](https://example.org) and {ref: eq-main}").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Link {
                    label: vec![Inline::Text("label".to_string())],
                    destination: "https://example.org".to_string(),
                },
                Inline::Text(" and ".to_string()),
                Inline::CrossRef("eq-main".to_string()),
            ]
        );
    }

    fn has_math(out: &[Inline]) -> bool {
        out.iter().any(|i| matches!(i, Inline::Math(_)))
    }

    #[test]
    fn test_dollar_inline_math_equals_paren_math() {
        let dollar = parse_inline(r"x $\frac{a}{b}$ y").unwrap();
        let paren = parse_inline(r"x \(\frac{a}{b}\) y").unwrap();
        assert_eq!(
            dollar,
            vec![
                Inline::Text("x ".to_string()),
                Inline::Math(r"\frac{a}{b}".to_string()),
                Inline::Text(" y".to_string()),
            ]
        );
        assert_eq!(dollar, paren);

        assert_eq!(
            parse_inline("$a$ and $b$").unwrap(),
            vec![
                Inline::Math("a".to_string()),
                Inline::Text(" and ".to_string()),
                Inline::Math("b".to_string()),
            ]
        );
        assert_eq!(
            parse_inline("($a$).").unwrap(),
            vec![
                Inline::Text("(".to_string()),
                Inline::Math("a".to_string()),
                Inline::Text(").".to_string()),
            ]
        );
    }

    #[test]
    fn test_currency_dollars_stay_text() {
        for input in [
            "It costs $5 to $10 per unit.",
            "Pay $ 5 now.",
            "A lone $ sign.",
            "$5$10",
            "a$b",
        ] {
            let out = parse_inline(input).unwrap();
            assert!(!has_math(&out), "{input:?} parsed as math: {out:?}");
            assert_eq!(plain_text(&out), input);
        }
        // Prices before real math must not pair with it: only the next
        // unescaped `$` may close an opener.
        let out = parse_inline("costs $5 to $10, inline $y^2$.").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Text("costs $5 to $10, inline ".to_string()),
                Inline::Math("y^2".to_string()),
                Inline::Text(".".to_string()),
            ]
        );
    }

    #[test]
    fn test_escaped_dollars() {
        assert_eq!(
            parse_inline(r"price \$5").unwrap(),
            vec![Inline::Text("price $5".to_string())]
        );
        let out = parse_inline(r"math $a \$ b$ here").unwrap();
        assert!(out.contains(&Inline::Math(r"a \$ b".to_string())), "{out:?}");
        let out = parse_inline(r"\$5 and $x$").unwrap();
        assert_eq!(
            out,
            vec![
                Inline::Text("$5 and ".to_string()),
                Inline::Math("x".to_string())
            ]
        );
    }

    #[test]
    fn test_midline_display_dollars_rejected() {
        let err = parse_inline("where $$x$$ holds").unwrap_err();
        assert!(err.0.contains("own line"), "{err:?}");
        let out = parse_inline(r"\$$x").unwrap();
        assert_eq!(out, vec![Inline::Text("$$x".to_string())]);
    }
}
