//! Which converter tools are installed, and where.
//!
//! A GUI app launched from Finder gets a bare `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`)
//! with no Homebrew in it, so PATH alone is useless there. Tools are looked up, in
//! order, in: Flick's own engines folder (download-on-demand), PATH, Homebrew,
//! MacPorts, /usr/local, and the app bundles LibreOffice / calibre install into.

use crate::graph::Engine;
use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Serialize, Debug, Default, Clone)]
pub struct EngineStatus {
    pub magick: bool,
    pub ffmpeg: bool,
    pub soffice: bool,
    pub pandoc: bool,
    pub pdftoppm: bool,
    pub mutool: bool,
    pub qpdf: bool,
    pub gs: bool,
    pub sevenzip: bool,
    /// Resolved absolute path per tool, for the UI's tooltip.
    pub found: BTreeMap<String, String>,
}

/// Flick's own engine folder: `~/Library/Application Support/Flick/engines/<tool>/`.
pub fn engines_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Flick/engines"))
}

/// Every directory a tool may live in, most specific first, without duplicates.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    };

    // 1. Our own downloaded engines (each tool in its own folder, binaries in `bin/` or at the root).
    if let Some(home) = engines_home() {
        if let Ok(rd) = std::fs::read_dir(&home) {
            for entry in rd.flatten() {
                let p = entry.path();
                push(p.join("bin"));
                push(p);
            }
        }
    }
    // 2. Whatever PATH the process did get.
    if let Some(path) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&path) {
            push(p);
        }
    }
    // 3. Where package managers and app bundles put things on a Mac.
    for d in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/opt/local/bin",
        "/opt/local/sbin",
        "/Applications/LibreOffice.app/Contents/MacOS",
        "/Applications/calibre.app/Contents/MacOS",
        "/usr/bin",
        "/bin",
    ] {
        push(PathBuf::from(d));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        push(h.join("Applications/LibreOffice.app/Contents/MacOS"));
        push(h.join(".cargo/bin"));
    }
    dirs
}

/// Absolute path of `bin`, or `None` when it isn't installed anywhere we look.
pub fn resolve(bin: &str) -> Option<PathBuf> {
    search_dirs().into_iter().map(|d| d.join(bin)).find(|f| f.is_file())
}

/// True when `bin` resolves somewhere in [`search_dirs`].
pub fn have(bin: &str) -> bool {
    resolve(bin).is_some()
}

/// PATH for child processes: the same search dirs, so a tool finds its own
/// helpers by name (ImageMagick calls Ghostscript to read PDFs, for example).
pub fn child_path() -> OsString {
    let dirs: Vec<PathBuf> = search_dirs().into_iter().filter(|d| d.is_dir()).collect();
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

pub fn detect_engines() -> EngineStatus {
    let mut st = EngineStatus::default();
    let mut note = |name: &str, bin: &str| -> bool {
        match resolve(bin) {
            Some(p) => {
                st.found.insert(name.to_string(), p.to_string_lossy().into_owned());
                true
            }
            None => false,
        }
    };
    st.magick = note("magick", Engine::Magick.bin());
    st.ffmpeg = note("ffmpeg", Engine::Ffmpeg.bin());
    st.soffice = note("soffice", Engine::Soffice.bin());
    st.pandoc = note("pandoc", Engine::Pandoc.bin());
    st.pdftoppm = note("pdftoppm", Engine::Pdftoppm.bin());
    st.mutool = note("mutool", Engine::Mutool.bin());
    st.qpdf = note("qpdf", Engine::Qpdf.bin());
    st.gs = note("gs", Engine::Gs.bin());
    st.sevenzip = note("sevenzip", "7z") || note("sevenzip", "7zz");
    st
}
