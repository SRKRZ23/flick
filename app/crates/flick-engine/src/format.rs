//! File-format taxonomy: extensions, categories, and the curated target /
//! tool lists that drive the three radial wheels in the UI.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Audio,
    Video,
    Pdf,
    Document,
    Spreadsheet,
    Slides,
    Archive,
    Text,
    Unknown,
}

/// Lowercased, alias-normalized extension taken from a path or bare filename.
/// Returns an empty string when there is no extension.
pub fn ext_of_path(path: &str) -> String {
    match path.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() && !e.contains(['/', '\\']) => normalize_ext(e),
        _ => String::new(),
    }
}

/// Collapse common aliases so the planner only ever reasons about one spelling.
pub fn normalize_ext(ext: &str) -> String {
    let e = ext.trim().trim_start_matches('.').to_lowercase();
    match e.as_str() {
        "jpeg" => "jpg".to_string(),
        "tif" => "tiff".to_string(),
        "heif" => "heic".to_string(),
        "markdown" | "mdown" => "md".to_string(),
        "htm" => "html".to_string(),
        _ => e,
    }
}

pub fn category_of(ext: &str) -> Category {
    match normalize_ext(ext).as_str() {
        "jpg" | "png" | "webp" | "heic" | "avif" | "tiff" | "bmp" | "gif" | "svg" | "ico" => {
            Category::Image
        }
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "aac" => Category::Audio,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "flv" => Category::Video,
        "pdf" => Category::Pdf,
        "docx" | "doc" | "odt" | "rtf" | "pages" => Category::Document,
        "xlsx" | "xls" | "ods" | "csv" | "tsv" => Category::Spreadsheet,
        "pptx" | "ppt" | "odp" | "key" => Category::Slides,
        "zip" | "7z" | "tar" | "gz" | "bz2" | "xz" | "rar" => Category::Archive,
        "md" | "html" | "txt" | "rst" | "org" | "tex" | "epub" => Category::Text,
        _ => Category::Unknown,
    }
}

/// Formats worth surfacing on the Convert wheel for a given source category.
/// The planner still filters these down to the ones actually reachable.
pub fn popular_targets(cat: Category) -> Vec<&'static str> {
    match cat {
        Category::Image => vec!["png", "jpg", "webp", "pdf", "heic", "tiff", "avif"],
        Category::Audio => vec!["mp3", "m4a", "wav", "flac", "ogg", "aiff"],
        Category::Video => vec!["mp4", "mov", "webm", "mkv", "gif", "mp3"],
        Category::Pdf => vec!["png", "jpg", "txt"],
        Category::Document => vec!["pdf", "docx", "odt", "txt", "html", "epub"],
        Category::Spreadsheet => vec!["pdf", "xlsx", "csv", "ods"],
        Category::Slides => vec!["pdf", "png", "jpg"],
        Category::Text => vec!["pdf", "docx", "html", "epub", "txt"],
        Category::Archive => vec![],
        Category::Unknown => vec![],
    }
}

/// The Command wheel: in-place edits / tools appropriate for the file type.
pub fn tools_for(cat: Category) -> Vec<&'static str> {
    match cat {
        Category::Image => vec!["Compress", "Resize", "Rotate", "Grayscale"],
        Category::Pdf => vec!["Compress", "Split pages", "First page \u{2192} PNG"],
        Category::Audio => vec!["Compress", "Trim 30s"],
        Category::Video => vec!["Compress", "Mute", "Poster frame", "\u{2192} GIF"],
        _ => vec!["Compress"],
    }
}
