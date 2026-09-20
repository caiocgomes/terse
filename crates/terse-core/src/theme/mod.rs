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
            "bfseries".to_string(),
            "bfseries".to_string(),
            "bfseries".to_string(),
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
