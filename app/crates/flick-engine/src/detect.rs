//! Which converter tools are actually installed on this machine. The UI uses
//! this to disable wheel slices it can't fulfil and to offer a `brew install`.

use crate::graph::Engine;
use serde::Serialize;

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
}

/// True when `bin` resolves to a file on PATH.
pub fn have(bin: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join(bin).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn detect_engines() -> EngineStatus {
    EngineStatus {
        magick: have(Engine::Magick.bin()),
        ffmpeg: have(Engine::Ffmpeg.bin()),
        soffice: have(Engine::Soffice.bin()),
        pandoc: have(Engine::Pandoc.bin()),
        pdftoppm: have(Engine::Pdftoppm.bin()),
        mutool: have(Engine::Mutool.bin()),
        qpdf: have(Engine::Qpdf.bin()),
        gs: have(Engine::Gs.bin()),
        sevenzip: have("7z") || have("7zz"),
    }
}
