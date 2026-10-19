//! Execution layer. Every converter runs as a separate child process — this
//! module only builds argument lists and chains temp files between hops. It
//! never loads a converter library in-process (keeps the MIT boundary clean).

use crate::detect::{child_path, resolve};
use crate::format::{category_of, ext_of_path, normalize_ext, Category};
use crate::graph::{all_edges, Engine};
use crate::planner::plan_route;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Debug, Clone)]
pub struct ConvertOutcome {
    pub ok: bool,
    pub outputs: Vec<String>,
    pub error: Option<String>,
}

impl ConvertOutcome {
    fn ok(outputs: Vec<PathBuf>) -> Self {
        Self {
            ok: true,
            outputs: outputs.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, outputs: Vec::new(), error: Some(msg.into()) }
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "output".into())
}

/// A non-clobbering destination path: `name.ext`, then `name 1.ext`, ...
fn unique(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let mut cand = dir.join(format!("{name}.{ext}"));
    let mut i = 1;
    while cand.exists() {
        cand = dir.join(format!("{name} {i}.{ext}"));
        i += 1;
    }
    cand
}

/// Homebrew formula that provides a tool (for the install hint).
fn brew_for(bin: &str) -> &'static str {
    match bin {
        "magick" => "imagemagick",
        "ffmpeg" => "ffmpeg",
        "soffice" => "libreoffice",
        "pandoc" => "pandoc",
        "pdftoppm" => "poppler",
        "mutool" => "mupdf-tools",
        "qpdf" => "qpdf",
        "gs" => "ghostscript",
        "7z" | "7zz" => "sevenzip",
        _ => "…",
    }
}

/// Homebrew's 7-Zip is `7zz` (sevenzip) or `7z` (p7zip) — take whichever exists.
fn seven_zip() -> &'static str {
    if resolve("7z").is_some() {
        "7z"
    } else {
        "7zz"
    }
}

/// Run a tool by its resolved absolute path. A GUI app launched from Finder has a
/// bare PATH, so both the lookup and the child's PATH (for helpers such as gs)
/// come from `detect::search_dirs`, never from PATH alone.
fn sh(bin: &str, args: &[&str]) -> Result<(), String> {
    let exe = resolve(bin).ok_or_else(|| {
        format!("`{bin}` is not installed. Install it with: brew install {}", brew_for(bin))
    })?;
    let out = Command::new(&exe)
        .args(args)
        .env("PATH", child_path())
        .output()
        .map_err(|e| format!("could not run `{bin}` ({e})"))?;
    if out.status.success() {
        Ok(())
    } else {
        let tail = String::from_utf8_lossy(&out.stderr);
        let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        Err(format!("{bin} failed: {last}"))
    }
}

/// On-disk extension a tool actually writes (pdftoppm emits .tif, not .tiff).
fn disk_ext(to: &str) -> &str {
    match to {
        "jpg" => "jpg",
        "tiff" => "tif",
        _ => "png",
    }
}

/// Collect `stem-*.ext` files a multi-page tool produced, sorted by name.
fn collect_prefixed(dir: &Path, stem: &str, ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut v = Vec::new();
    let needle = format!("{stem}-");
    let suffix = format!(".{ext}");
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(&needle) && name.ends_with(&suffix) {
                v.push(p);
            }
        }
    }
    v.sort();
    if v.is_empty() {
        Err("the tool produced no output pages".into())
    } else {
        Ok(v)
    }
}

/// Run a single converter edge: `input` -> a file (or files) of format `to`.
fn run_step(engine: Engine, input: &Path, to: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
    let st = stem(input);
    let inp = input.to_str().ok_or("input path is not valid UTF-8")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    match engine {
        Engine::Magick => {
            let out = unique(dir, &st, to);
            sh("magick", &[inp, out.to_str().unwrap()])?;
            Ok(vec![out])
        }
        Engine::Ffmpeg => {
            let out = unique(dir, &st, to);
            let from = ext_of_path(inp);
            let from_video =
                matches!(from.as_str(), "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "flv");
            let to_audio = matches!(
                to,
                "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "aac"
            );
            let mut args: Vec<&str> = vec!["-y", "-i", inp];
            if from_video && to_audio {
                args.push("-vn"); // drop the video stream when extracting audio
            }
            let out_s = out.to_string_lossy().into_owned();
            args.push(&out_s);
            sh("ffmpeg", &args)?;
            Ok(vec![out])
        }
        Engine::Soffice => {
            // Keep a dedicated profile dir so a running LibreOffice instance
            // doesn't block the headless conversion.
            let profile = std::env::temp_dir().join("flick-soffice-profile");
            let env_arg = format!("-env:UserInstallation=file://{}", profile.to_string_lossy());
            sh(
                "soffice",
                &["--headless", &env_arg, "--convert-to", to, "--outdir", dir.to_str().unwrap(), inp],
            )?;
            let produced = dir.join(format!("{st}.{to}"));
            if produced.exists() {
                Ok(vec![produced])
            } else {
                Err("LibreOffice produced no output (unsupported target?)".into())
            }
        }
        Engine::Pandoc => {
            let out = unique(dir, &st, to);
            sh("pandoc", &[inp, "-o", out.to_str().unwrap()])?;
            Ok(vec![out])
        }
        Engine::Pdftoppm => {
            let fmt = match to {
                "jpg" => "-jpeg",
                "tiff" => "-tiff",
                _ => "-png",
            };
            let prefix = dir.join(&st);
            sh("pdftoppm", &[fmt, "-r", "150", inp, prefix.to_str().unwrap()])?;
            collect_prefixed(dir, &st, disk_ext(to))
        }
        Engine::Mutool => {
            let out = unique(dir, &st, to);
            sh("mutool", &["convert", "-o", out.to_str().unwrap(), inp])?;
            Ok(vec![out])
        }
        Engine::Qpdf | Engine::Gs | Engine::SevenZip => {
            Err("this engine is only used for tools/archives, not format steps".into())
        }
    }
}

/// Plan a route and run it, chaining intermediate files through a temp dir.
pub fn convert_file(src: &Path, to_ext: &str, out_dir: &Path) -> ConvertOutcome {
    let from = ext_of_path(&src.to_string_lossy());
    if from.is_empty() {
        return ConvertOutcome::err("file has no extension");
    }
    let to = normalize_ext(to_ext);
    let edges = all_edges();
    let route = match plan_route(&from, &to, &edges, 3) {
        Some(r) if !r.is_empty() => r,
        Some(_) => return ConvertOutcome::err("the file is already that format"),
        None => return ConvertOutcome::err(format!("no conversion route {from} \u{2192} {to}")),
    };

    if let Err(e) = std::fs::create_dir_all(out_dir) {
        return ConvertOutcome::err(e.to_string());
    }
    let tmp = std::env::temp_dir().join(format!("flick-{}", std::process::id()));

    let mut current = vec![src.to_path_buf()];
    for (i, step) in route.iter().enumerate() {
        let last = i + 1 == route.len();
        let dir = if last { out_dir } else { &tmp };
        let mut next = Vec::new();
        for inp in &current {
            match run_step(step.engine, inp, &step.to, dir) {
                Ok(mut outs) => next.append(&mut outs),
                Err(e) => return ConvertOutcome::err(e),
            }
        }
        current = next;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    ConvertOutcome::ok(current)
}

/// Run a Command-wheel tool on one file.
pub fn run_tool(src: &Path, tool: &str, out_dir: &Path) -> ConvertOutcome {
    let st = stem(src);
    let inp = match src.to_str() {
        Some(s) => s,
        None => return ConvertOutcome::err("input path is not valid UTF-8"),
    };
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        return ConvertOutcome::err(e.to_string());
    }
    let from = ext_of_path(inp);

    let result: Result<Vec<PathBuf>, String> = (|| match tool {
        "Compress" => match category_of(&from) {
            Category::Image => {
                let o = unique(out_dir, &format!("{st} compressed"), &from);
                sh("magick", &[inp, "-quality", "60", o.to_str().unwrap()])?;
                Ok(vec![o])
            }
            Category::Pdf => {
                let o = unique(out_dir, &format!("{st} compressed"), "pdf");
                sh(
                    "gs",
                    &[
                        "-sDEVICE=pdfwrite",
                        "-dCompatibilityLevel=1.4",
                        "-dPDFSETTINGS=/ebook",
                        "-dNOPAUSE",
                        "-dQUIET",
                        "-dBATCH",
                        &format!("-sOutputFile={}", o.to_string_lossy()),
                        inp,
                    ],
                )?;
                Ok(vec![o])
            }
            Category::Video => {
                let o = unique(out_dir, &format!("{st} compressed"), "mp4");
                sh("ffmpeg", &["-y", "-i", inp, "-vcodec", "libx264", "-crf", "28", o.to_str().unwrap()])?;
                Ok(vec![o])
            }
            Category::Audio => {
                let o = unique(out_dir, &format!("{st} compressed"), "mp3");
                sh("ffmpeg", &["-y", "-i", inp, "-b:a", "128k", o.to_str().unwrap()])?;
                Ok(vec![o])
            }
            _ => Err("compress isn't available for this file type".into()),
        },
        "Resize" => {
            let o = unique(out_dir, &format!("{st} resized"), &from);
            sh("magick", &[inp, "-resize", "50%", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "Rotate" => {
            let o = unique(out_dir, &format!("{st} rotated"), &from);
            sh("magick", &[inp, "-rotate", "90", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "Grayscale" => {
            let o = unique(out_dir, &format!("{st} gray"), &from);
            sh("magick", &[inp, "-colorspace", "Gray", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "First page \u{2192} PNG" => {
            let prefix = out_dir.join(&st);
            sh("pdftoppm", &["-png", "-r", "150", "-f", "1", "-l", "1", inp, prefix.to_str().unwrap()])?;
            collect_prefixed(out_dir, &st, "png")
        }
        "Split pages" => {
            let pat = out_dir.join(format!("{st}-%d.pdf"));
            sh("qpdf", &["--split-pages", inp, pat.to_str().unwrap()])?;
            collect_prefixed(out_dir, &st, "pdf")
        }
        "Mute" => {
            let o = unique(out_dir, &format!("{st} muted"), &from);
            sh("ffmpeg", &["-y", "-i", inp, "-an", "-vcodec", "copy", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "Poster frame" => {
            let o = unique(out_dir, &format!("{st} poster"), "png");
            sh("ffmpeg", &["-y", "-i", inp, "-vframes", "1", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "\u{2192} GIF" => {
            let o = unique(out_dir, &st, "gif");
            sh("ffmpeg", &["-y", "-i", inp, o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        "Trim 30s" => {
            let o = unique(out_dir, &format!("{st} 30s"), &from);
            sh("ffmpeg", &["-y", "-i", inp, "-t", "30", "-c", "copy", o.to_str().unwrap()])?;
            Ok(vec![o])
        }
        other => Err(format!("unknown tool: {other}")),
    })();

    match result {
        Ok(o) => ConvertOutcome::ok(o),
        Err(e) => ConvertOutcome::err(e),
    }
}

/// Pack several files, unconverted, into one archive.
pub fn zip_together(paths: &[String], out_dir: &Path, fmt: &str) -> ConvertOutcome {
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        return ConvertOutcome::err(e.to_string());
    }
    let fmt = if fmt.is_empty() { "zip" } else { fmt };
    let o = unique(out_dir, "Flick archive", fmt);
    let mut args: Vec<&str> = vec!["a", o.to_str().unwrap()];
    for p in paths {
        args.push(p);
    }
    match sh(seven_zip(), &args) {
        Ok(()) => ConvertOutcome::ok(vec![o]),
        Err(e) => ConvertOutcome::err(e),
    }
}

/// Extract an archive into a sibling folder.
pub fn extract(path: &str, out_dir: &Path) -> ConvertOutcome {
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".into());
    let dest = out_dir.join(format!("{name} extracted"));
    if let Err(e) = std::fs::create_dir_all(&dest) {
        return ConvertOutcome::err(e.to_string());
    }
    match sh(seven_zip(), &["x", path, &format!("-o{}", dest.to_string_lossy()), "-y"]) {
        Ok(()) => ConvertOutcome::ok(vec![dest]),
        Err(e) => ConvertOutcome::err(e),
    }
}
