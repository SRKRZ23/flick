//! The converter graph: which tool turns format A into format B, and how
//! "expensive" that edge is. The RoutePlanner searches this graph, so new
//! capabilities are added here, in one place, rather than in the UI.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Engine {
    Magick,
    Ffmpeg,
    Soffice,
    Pandoc,
    Pdftoppm,
    Mutool,
    Qpdf,
    Gs,
    SevenZip,
}

impl Engine {
    /// The executable name looked up on PATH.
    pub fn bin(&self) -> &'static str {
        match self {
            Engine::Magick => "magick",
            Engine::Ffmpeg => "ffmpeg",
            Engine::Soffice => "soffice",
            Engine::Pandoc => "pandoc",
            Engine::Pdftoppm => "pdftoppm",
            Engine::Mutool => "mutool",
            Engine::Qpdf => "qpdf",
            Engine::Gs => "gs",
            Engine::SevenZip => "7z",
        }
    }

    /// Homebrew formula that provides this tool (for guided install).
    pub fn brew(&self) -> &'static str {
        match self {
            Engine::Magick => "imagemagick",
            Engine::Ffmpeg => "ffmpeg",
            Engine::Soffice => "libreoffice",
            Engine::Pandoc => "pandoc",
            Engine::Pdftoppm => "poppler",
            Engine::Mutool => "mupdf-tools",
            Engine::Qpdf => "qpdf",
            Engine::Gs => "ghostscript",
            Engine::SevenZip => "sevenzip",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub engine: Engine,
    pub cost: u32,
}

/// Build the full edge set. Costs are tuned so the planner prefers a single
/// high-fidelity tool over a longer chain (e.g. image->image via magick is
/// cheaper per hop than routing through an office conversion).
pub fn all_edges() -> Vec<Edge> {
    let mut e: Vec<Edge> = Vec::new();
    {
        let mut push = |from: &str, to: &str, engine: Engine, cost: u32| {
            e.push(Edge { from: from.into(), to: to.into(), engine, cost })
        };

        // Images (raster) — any-to-any via ImageMagick, plus image -> PDF.
        let raster = ["jpg", "png", "webp", "heic", "avif", "tiff", "bmp", "gif"];
        for &a in &raster {
            for &b in &raster {
                if a != b {
                    push(a, b, Engine::Magick, 2);
                }
            }
            push(a, "pdf", Engine::Magick, 2);
        }

        // Audio — any-to-any via ffmpeg.
        let audio = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff"];
        for &a in &audio {
            for &b in &audio {
                if a != b {
                    push(a, b, Engine::Ffmpeg, 2);
                }
            }
        }

        // Video — any-to-any via ffmpeg (gif included as a video target).
        let video = ["mp4", "mov", "mkv", "webm", "avi", "gif"];
        for &a in &video {
            for &b in &video {
                if a != b {
                    push(a, b, Engine::Ffmpeg, 3);
                }
            }
        }
        // Video -> audio (extract the soundtrack).
        for &v in &["mp4", "mov", "mkv", "webm", "avi"] {
            for &a in &["mp3", "m4a", "wav"] {
                push(v, a, Engine::Ffmpeg, 3);
            }
        }

        // Office / documents / spreadsheets / slides -> PDF via LibreOffice.
        let office = [
            "docx", "doc", "odt", "rtf", "xlsx", "xls", "ods", "csv", "pptx", "ppt", "odp",
        ];
        for &o in &office {
            push(o, "pdf", Engine::Soffice, 4);
        }

        // Markup / text / ebooks — pairwise via Pandoc (curated, not every pair).
        let markup_src = ["md", "html", "rst", "org", "docx", "epub", "rtf", "txt"];
        let markup_dst = ["md", "html", "docx", "epub", "rtf", "txt"];
        for &a in &markup_src {
            for &b in &markup_dst {
                if a != b {
                    push(a, b, Engine::Pandoc, 3);
                }
            }
        }

        // PDF -> raster (poppler) and PDF -> text (mupdf).
        for &t in &["png", "jpg", "tiff"] {
            push("pdf", t, Engine::Pdftoppm, 3);
        }
        push("pdf", "txt", Engine::Mutool, 3);
    }
    e
}
