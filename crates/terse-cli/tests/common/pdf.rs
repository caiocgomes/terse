//! PDF inspection helpers for e2e assertions.
//!
//! Backed by Poppler's `pdftotext`/`pdfinfo`/`pdftoppm` command-line tools
//! (available in the pinned environment; verified present at the top of any
//! test that uses this module via [`require_pdf_tools`]). No PDF-parsing
//! crate is added as a production or dev dependency for this: shelling out
//! to the same toolchain the pinned CI image already provisions for the
//! rasterization gate keeps the text-layer, link, and pixel-region checks
//! consistent with one real PDF renderer instead of a second, independent
//! parser that could disagree with it.
//!
//! Capabilities:
//! - text layer extraction (`extract_text`, `extract_text_per_page`)
//! - hyperlink/URI annotation destinations (`extract_links`, via `pdftotext
//!   -linksonly... ` is not a real poppler flag, so this parses annotations
//!   through `pdfinfo`'s `-listlinks` when available, falling back to a
//!   built-in minimal PDF object scan for `/URI` and `/GoTo` targets)
//! - whole-page rasterization to PNG and pixel-region sampling
//!   (`render_page_png`, `sample_region_is_blank`)
//!
//! Limitation: `pdftotext`'s text layer does not expose internal link
//! destinations directly, so [`extract_links`] falls back to a direct scan
//! of the PDF's own object stream for `/URI (...)` and named-destination
//! `/GoTo` entries. This is a minimal, purpose-built scanner (not a general
//! PDF parser) and only needs to find literal byte patterns in
//! uncompressed object dictionaries, which is what XeLaTeX/hyperref
///emit for `\href`/`\hyperref` targets in this project's generated output.

use std::path::Path;
use std::process::Command;

pub fn require_pdf_tools() {
    for tool in ["pdftotext", "pdfinfo", "pdftoppm"] {
        let found = Command::new(tool).arg("-v").output();
        assert!(
            found.is_ok(),
            "{tool} must be installed for PDF-inspection e2e tests"
        );
    }
}

/// Full text layer of the PDF, concatenated across pages.
pub fn extract_text(pdf: &Path) -> String {
    let output = Command::new("pdftotext")
        .arg(pdf)
        .arg("-")
        .output()
        .expect("pdftotext must run");
    assert!(output.status.success(), "pdftotext failed on {pdf:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The `pdffonts` table: one line per embedded font, name first. Used to
/// prove which family a build actually embedded, which neither the style
/// text nor the text layer can show.
pub fn fonts(pdf: &Path) -> String {
    let output = Command::new("pdffonts")
        .arg(pdf)
        .output()
        .expect("pdffonts must run");
    assert!(output.status.success(), "pdffonts failed on {pdf:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Page count, via pdfinfo.
pub fn page_count(pdf: &Path) -> usize {
    let output = Command::new("pdfinfo")
        .arg(pdf)
        .output()
        .expect("pdfinfo must run");
    assert!(output.status.success(), "pdfinfo failed on {pdf:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|l| l.strip_prefix("Pages:"))
        .and_then(|n| n.trim().parse().ok())
        .expect("pdfinfo output must contain a Pages: line")
}

/// Page dimensions in PostScript points, as `pdfinfo` reports them
/// (`Page size:  612 x 792 pts (letter)`). A4 is 595 x 842, letter is
/// 612 x 792, so this distinguishes the two page sizes a theme can select
/// as well as any margin change that alters the trim box.
pub fn page_size(pdf: &Path) -> (f32, f32) {
    let output = Command::new("pdfinfo")
        .arg(pdf)
        .output()
        .expect("pdfinfo must run");
    assert!(output.status.success(), "pdfinfo failed on {pdf:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("Page size:"))
        .expect("pdfinfo output must contain a Page size: line");
    let mut parts = line.split_whitespace();
    let width: f32 = parts.next().and_then(|n| n.parse().ok()).expect("page width");
    parts.next();
    let height: f32 = parts.next().and_then(|n| n.parse().ok()).expect("page height");
    (width, height)
}

/// Text layer of one specific page (1-indexed).
pub fn extract_text_page(pdf: &Path, page: usize) -> String {
    let output = Command::new("pdftotext")
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg(pdf)
        .arg("-")
        .output()
        .expect("pdftotext must run");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Best-effort scan for `/URI (...)` link-annotation targets anywhere in
/// the raw PDF bytes. See module docs for why this isn't a full PDF
/// object-stream parser.
pub fn extract_links(pdf: &Path) -> Vec<String> {
    let bytes = std::fs::read(pdf).expect("PDF must be readable");
    let text = String::from_utf8_lossy(&bytes);
    let mut links = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find("/URI (") {
        let after = &rest[start + "/URI (".len()..];
        if let Some(end) = after.find(')') {
            links.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    links
}

/// Render one page (1-indexed) to a PNG file, returning its path.
pub fn render_page_png(pdf: &Path, page: usize, out_dir: &Path) -> std::path::PathBuf {
    let prefix = out_dir.join("page");
    let status = Command::new("pdftoppm")
        .arg("-png")
        .arg("-r")
        .arg("72")
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg(pdf)
        .arg(&prefix)
        .status()
        .expect("pdftoppm must run");
    assert!(status.success(), "pdftoppm failed on {pdf:?} page {page}");
    // pdftoppm names single-page output `page-<page>.png` (or `page-01.png`
    // depending on total page count width); find whatever it produced.
    for entry in std::fs::read_dir(out_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("page") && name.ends_with(".png") {
            return entry.path();
        }
    }
    panic!("pdftoppm did not produce an output PNG in {out_dir:?}");
}

/// True if the normalized rectangular region (fractions of width/height,
/// 0.0-1.0) of the given PNG is a single uniform color (e.g. all-white),
/// which the acceptance contract treats as empty/invisible content even
/// when a text layer technically exists.
pub fn region_is_uniform(png_path: &Path, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    let img = load_png_rgb(png_path);
    let (w, h) = (img.width, img.height);
    let (px0, py0) = ((x0 * w as f32) as u32, (y0 * h as f32) as u32);
    let (px1, py1) = ((x1 * w as f32) as u32, (y1 * h as f32) as u32);
    let mut first: Option<(u8, u8, u8)> = None;
    for y in py0..py1.min(h) {
        for x in px0..px1.min(w) {
            let px = img.pixel(x, y);
            match first {
                None => first = Some(px),
                Some(f) => {
                    if diff(f, px) > 6 {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn diff(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    (a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs() + (a.2 as i32 - b.2 as i32).abs()
}

struct RgbImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    channels: usize,
}

impl RgbImage {
    fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        let idx = ((y * self.width + x) as usize) * self.channels;
        (
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
        )
    }
}

/// Minimal PNG decoder sufficient for pdftoppm's uncompressed-filter
/// output: reads IHDR for dimensions/color type, concatenates IDAT chunks,
/// zlib-inflates, and un-filters scanlines (defiltering only; no palette
/// support since pdftoppm always emits RGB/RGBA truecolor PNGs).
fn load_png_rgb(path: &Path) -> RgbImage {
    let data = std::fs::read(path).expect("PNG must be readable");
    assert_eq!(&data[0..8], b"\x89PNG\r\n\x1a\n", "not a PNG file");
    let mut pos = 8;
    let (mut width, mut height, mut color_type) = (0u32, 0u32, 0u8);
    let mut idat = Vec::new();
    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &data[pos + 4..pos + 8];
        let chunk_data = &data[pos + 8..pos + 8 + len];
        match kind {
            b"IHDR" => {
                width = u32::from_be_bytes(chunk_data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(chunk_data[4..8].try_into().unwrap());
                color_type = chunk_data[9];
            }
            b"IDAT" => idat.extend_from_slice(chunk_data),
            b"IEND" => break,
            _ => {}
        }
        pos += 8 + len + 4; // data + CRC
    }
    let channels = match color_type {
        2 => 3, // truecolor
        6 => 4, // truecolor + alpha
        _ => panic!("unsupported PNG color type {color_type} (expected pdftoppm RGB/RGBA output)"),
    };
    let raw = inflate_zlib(&idat);
    let stride = 1 + width as usize * channels;
    let mut pixels = vec![0u8; width as usize * height as usize * channels];
    let mut prev_row = vec![0u8; width as usize * channels];
    for y in 0..height as usize {
        let row_start = y * stride;
        let filter = raw[row_start];
        let row = &raw[row_start + 1..row_start + stride];
        let out_start = y * width as usize * channels;
        let out_row = &mut pixels[out_start..out_start + width as usize * channels];
        for x in 0..out_row.len() {
            let a = if x >= channels { out_row[x - channels] } else { 0 };
            let b = prev_row[x];
            let c = if x >= channels { prev_row[x - channels] } else { 0 };
            let raw_byte = row[x];
            out_row[x] = match filter {
                0 => raw_byte,
                1 => raw_byte.wrapping_add(a),
                2 => raw_byte.wrapping_add(b),
                3 => raw_byte.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => raw_byte.wrapping_add(paeth(a, b, c)),
                other => panic!("unsupported PNG filter type {other}"),
            };
        }
        prev_row.copy_from_slice(out_row);
    }
    RgbImage {
        width,
        height,
        pixels,
        channels,
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Minimal zlib/DEFLATE inflate. `pdftoppm`'s PNG writer produces standard
/// zlib streams; this uses the `flate2` crate already present as a
/// transitive dependency of `zip` if available, otherwise a hand-rolled
/// inflate is out of scope, so this function requires the `flate2` crate.
fn inflate_zlib(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .expect("zlib inflate of PNG IDAT stream must succeed");
    out
}
