//! Bounded theme resolution: a closed `.theme` selector/property schema
//! (`parse`/`resolve`) compiled into an immutable [`ResolvedTheme`] that
//! only supplies typed presentation values to the trusted LaTeX style
//! templates. Themes never see or mutate authored semantic nodes.

pub mod parse;
pub mod resolve;

use crate::source::SourceFile;
pub use resolve::ThemeResolveError;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub name: String,
    pub page_size: String,
    pub page_margin_cm: f64,
    pub page_columns: u8,
    /// A TeX-distributed font package that loads its fonts by filename
    /// (e.g. `libertinus-otf`), never a bare font family name: family-name
    /// lookup depends on the host's fontconfig database, which a fresh
    /// TeX installation may not populate with TeX-tree fonts.
    pub body_font: String,
    pub body_color: String,
    pub heading_weight: [String; 3],
    pub heading_numbering: [String; 3],
    pub figure_align: String,
    pub figure_default_width_pct: u8,
    pub figure_wide_width_pct: u8,
    pub figure_placement: String,
    pub citation_style: String,
    /// `paper` keeps the title material inline above the body; `cover`
    /// gives it its own page, ending with `\clearpage`. A cover is a layout
    /// decision, not a font size, which is why the body wraps all of its
    /// title macros in one `TerseTitleBlock` environment the style owns.
    pub title_layout: String,
    pub title_align: String,
    /// One `amsthm` style per theorem-like kind, in the semantic model's
    /// kind order (theorem, proposition, lemma, definition, example,
    /// remark). A base `theorem` rule sets all six; a `theorem[kind=...]`
    /// rule overrides one, which is the inheritance the precedence
    /// requirement demands.
    pub theorem_style: [String; 6],
    pub table_padding: f64,
    pub table_rules: String,
    pub table_header: String,
    pub bibliography_size: String,
    pub bibliography_item_spacing_em: f64,
    pub watermark_kind: String,
    pub watermark_opacity: f64,
    pub watermark_angle: f64,
    /// Theme-relative logo path, validated as a non-external resource by
    /// [`resolve`]; the CLI resolves and canonicalizes it against the
    /// declaring `.theme` file's directory before copying it.
    pub logo_path: Option<String>,
    pub logo_width_pct: u8,
}

/// The versioned compiler defaults every theme resolves on top of, before
/// any `.theme` rule is applied.
fn compiler_defaults(name: &str) -> ResolvedTheme {
    ResolvedTheme {
        name: name.to_string(),
        page_size: "a4".to_string(),
        page_margin_cm: 2.5,
        page_columns: 1,
        body_font: "libertinus-otf".to_string(),
        body_color: "000000".to_string(),
        heading_weight: [
            "bold".to_string(),
            "bold".to_string(),
            "bold".to_string(),
        ],
        heading_numbering: [
            "decimal".to_string(),
            "decimal".to_string(),
            "decimal".to_string(),
        ],
        figure_align: "center".to_string(),
        figure_default_width_pct: 75,
        figure_wide_width_pct: 100,
        figure_placement: "here".to_string(),
        citation_style: "author-year".to_string(),
        title_layout: "paper".to_string(),
        // Left, because that is what every document rendered before the
        // `title` component existed: making a setting real must not
        // silently restyle documents that never asked for a change. A theme
        // opts into centering.
        title_align: "left".to_string(),
        theorem_style: [
            "plain".to_string(),
            "plain".to_string(),
            "plain".to_string(),
            "definition".to_string(),
            "definition".to_string(),
            "remark".to_string(),
        ],
        table_padding: 1.0,
        table_rules: "booktabs".to_string(),
        table_header: "bold".to_string(),
        bibliography_size: "normal".to_string(),
        bibliography_item_spacing_em: 0.5,
        watermark_kind: "none".to_string(),
        watermark_opacity: 0.0,
        watermark_angle: 45.0,
        logo_path: None,
        logo_width_pct: 20,
    }
}

/// The initial academic theme's immutable resolved defaults, used to
/// generate the milestone-1 scaffold's `terse-style.sty` when no `.theme`
/// file overrides them yet.
pub fn academic() -> ResolvedTheme {
    compiler_defaults("academic")
}

/// Parses and resolves a `.theme` source file into a [`ResolvedTheme`],
/// starting from the versioned compiler defaults for `name`. This is the
/// only path from authored theme bytes to presentation settings; it never
/// touches the filesystem itself (the caller already loaded `source`).
pub fn resolve_theme(name: &str, source: &SourceFile) -> Result<ResolvedTheme, ThemeError> {
    let rules = parse::parse(source).map_err(ThemeError::Parse)?;
    resolve::resolve(name, compiler_defaults(name), &rules).map_err(ThemeError::Resolve)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThemeError {
    Parse(parse::ThemeParseError),
    Resolve(Vec<ThemeResolveError>),
}

#[cfg(test)]
mod tests;
