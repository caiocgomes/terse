//! Cited-only BibLaTeX serialization: normalized [`NormalizedRecord`]s
//! (from an already-resolved lock, via [`crate::references::bind`]) become
//! a deterministic `.bib` file. Display/citation-order in the compiled
//! document is independent of this file's order — BibLaTeX is configured
//! with `sorting=none` (citation order), so `.bib` may sort by key while
//! the printed bibliography follows first-appearance order.

use std::collections::BTreeMap;

use super::escape::escape_bib_value;
use crate::references::record::{NormalizedRecord, PersonName, WorkType};

fn entry_type(work_type: WorkType) -> &'static str {
    match work_type {
        WorkType::JournalArticle => "article",
        WorkType::ConferencePaper => "inproceedings",
        WorkType::Preprint => "unpublished",
        WorkType::Book => "book",
        WorkType::BookChapter => "incollection",
        WorkType::Report => "report",
        WorkType::Thesis => "thesis",
        WorkType::Dataset => "dataset",
        WorkType::Software => "software",
        WorkType::Misc => "misc",
    }
}

/// The BibLaTeX field that holds a work's container (its journal,
/// conference proceedings, or containing book), by work type. Types with
/// no natural container (books, reports, theses, datasets, software) omit
/// it even if `container` happens to be set, since biblatex has no such
/// field on those entry types.
fn container_field(work_type: WorkType) -> Option<&'static str> {
    match work_type {
        WorkType::JournalArticle => Some("journaltitle"),
        WorkType::ConferencePaper | WorkType::BookChapter => Some("booktitle"),
        _ => None,
    }
}

fn format_name(name: &PersonName) -> String {
    match name {
        PersonName::Structured { family, given } => {
            format!("{}, {}", escape_bib_value(family), escape_bib_value(given))
        }
        // Braced so BibLaTeX/Biber never tries to split an unparsed full
        // name or an organization into family/given parts of its own.
        PersonName::Unparsed { name } => format!("{{{}}}", escape_bib_value(name)),
        PersonName::Organization { name } => format!("{{{}}}", escape_bib_value(name)),
    }
}

fn format_name_list(names: &[PersonName]) -> String {
    names.iter().map(format_name).collect::<Vec<_>>().join(" and ")
}

/// Serializes one cited work into a single `@type{key, ...}` BibLaTeX
/// entry. The caller guarantees `record.validate_effective()` already
/// succeeded, so a missing title/work-type/contributor set here would be
/// a caller bug, not a normal input to handle gracefully.
fn serialize_entry(alias: &str, record: &NormalizedRecord) -> String {
    let work_type = record.work_type.expect("effective record has a work type");
    let mut fields: Vec<(&str, String)> = Vec::new();

    if let Some(title) = &record.title {
        fields.push(("title", format!("{{{}}}", escape_bib_value(title))));
    }
    if !record.authors.is_empty() {
        fields.push(("author", format!("{{{}}}", format_name_list(&record.authors))));
    }
    if !record.editors.is_empty() {
        fields.push(("editor", format!("{{{}}}", format_name_list(&record.editors))));
    }
    if let Some(field) = container_field(work_type) {
        if let Some(container) = &record.container {
            fields.push((field, format!("{{{}}}", escape_bib_value(container))));
        }
    }
    if let Some(date) = &record.date {
        fields.push(("date", format!("{{{}}}", escape_bib_value(date))));
    }
    if let Some(publisher) = &record.publisher {
        fields.push(("publisher", format!("{{{}}}", escape_bib_value(publisher))));
    }
    if let Some(volume) = &record.volume {
        fields.push(("volume", format!("{{{}}}", escape_bib_value(volume))));
    }
    if let Some(issue) = &record.issue {
        fields.push(("number", format!("{{{}}}", escape_bib_value(issue))));
    }
    if let Some(pages) = &record.pages {
        fields.push(("pages", format!("{{{}}}", escape_bib_value(pages))));
    }

    let body: String = fields
        .iter()
        .map(|(k, v)| format!("  {k} = {v},\n"))
        .collect();
    format!("@{}{{{},\n{}}}\n", entry_type(work_type), alias, body)
}

/// Generates the complete `.bib` text for exactly the cited, authorized
/// works (`cited` is the intersection of authored citations and bound
/// aliases, computed by the caller). Entries are ordered by alias (a
/// `BTreeMap` iterates sorted), independent of any citation/display
/// order in the compiled document.
pub fn generate_bib(cited: &BTreeMap<String, NormalizedRecord>) -> String {
    let mut out = String::new();
    for (alias, record) in cited {
        out.push_str(&serialize_entry(alias, record));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(title: &str) -> NormalizedRecord {
        NormalizedRecord {
            title: Some(title.to_string()),
            work_type: Some(WorkType::JournalArticle),
            authors: vec![PersonName::Structured { family: "Robins".to_string(), given: "James".to_string() }],
            container: Some("Epidemiology".to_string()),
            date: Some("1986".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_bib_order_vs_display_order() {
        // `.bib` serializes by key regardless of citation order in the
        // document; display order in the compiled PDF is a property of
        // `sorting=none` (citation order) set on the `biblatex` package
        // in `generate_style`, entirely independent of this file.
        let mut cited = BTreeMap::new();
        cited.insert("zeta".to_string(), article("Zeta work"));
        cited.insert("alpha".to_string(), article("Alpha work"));
        let bib = generate_bib(&cited);
        assert!(bib.find("@article{alpha").unwrap() < bib.find("@article{zeta").unwrap());
    }

    #[test]
    fn test_bib_is_cited_only_and_key_sorted() {
        let mut cited = BTreeMap::new();
        cited.insert("zeta".to_string(), article("Zeta work"));
        cited.insert("alpha".to_string(), article("Alpha work"));
        let bib = generate_bib(&cited);
        let alpha_pos = bib.find("@article{alpha").unwrap();
        let zeta_pos = bib.find("@article{zeta").unwrap();
        assert!(alpha_pos < zeta_pos, "alpha must serialize before zeta (key order)");
    }

    #[test]
    fn test_bib_excludes_uncited_works() {
        let cited = BTreeMap::new();
        assert_eq!(generate_bib(&cited), "");
    }

    #[test]
    fn test_unparsed_and_organization_names_are_braced_literals() {
        let mut record = article("Report");
        record.authors = vec![PersonName::Unparsed { name: "J. Robins".to_string() }];
        let text = serialize_entry("who", &record);
        assert!(text.contains("author = {{J. Robins}}"));
    }

    #[test]
    fn test_container_field_depends_on_work_type() {
        let text = serialize_entry("robins1986", &article("T"));
        assert!(text.contains("journaltitle = {Epidemiology}"));

        let mut book = article("T");
        book.work_type = Some(WorkType::Book);
        let text = serialize_entry("book1", &book);
        assert!(!text.contains("journaltitle"));
    }
}
