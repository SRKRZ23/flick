//! End-to-end smoke test: generate real inputs with the installed tools, then
//! drive the engine through single- and multi-hop routes and verify every
//! output file exists and is non-empty. Run with:
//!     cargo run --example smoke
//! Exits non-zero if any case fails.

use flick_engine::*;
use std::path::{Path, PathBuf};
use std::process::Command;

// Resolve through the engine's own lookup so this test also proves the
// Finder-launch case (bare PATH without Homebrew): run it with
//   env -i PATH=/usr/bin:/bin HOME=$HOME target/debug/examples/smoke
fn run(bin: &str, args: &[&str]) {
    let exe = resolve(bin).unwrap_or_else(|| panic!("{bin} not found in any search dir"));
    let status = Command::new(&exe)
        .args(args)
        .env("PATH", std::env::join_paths(search_dirs()).unwrap())
        .status()
        .unwrap_or_else(|e| panic!("could not run {bin}: {e}"));
    assert!(status.success(), "{bin} failed while preparing a sample input");
}

fn nonempty(p: &str) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
}

fn main() {
    let dir = std::env::temp_dir().join("flick-smoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();

    // --- prepare real sample inputs using the installed tools ---
    let png = dir.join("photo.png");
    run(
        "magick",
        &["-size", "640x400", "gradient:#6a7cff-#14b8a6", png.to_str().unwrap()],
    );
    let wav = dir.join("tone.wav");
    run("ffmpeg", &["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", wav.to_str().unwrap()]);
    let mp4 = dir.join("clip.mp4");
    run(
        "ffmpeg",
        &[
            "-y",
            "-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=2",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
            "-pix_fmt", "yuv420p", "-shortest", mp4.to_str().unwrap(),
        ],
    );
    let md = dir.join("notes.md");
    std::fs::write(&md, "# Flick\n\nHello **world** from a multi-hop route.\n\n- one\n- two\n").unwrap();
    let docx = dir.join("notes.docx");
    run("pandoc", &[md.to_str().unwrap(), "-o", docx.to_str().unwrap()]);
    // A real PDF to exercise the PDF -> raster / text edges.
    let pdf = dir.join("doc.pdf");
    run(
        "soffice",
        &[
            "--headless",
            "-env:UserInstallation=file:///tmp/flick-soffice-profile",
            "--convert-to", "pdf", "--outdir", dir.to_str().unwrap(), docx.to_str().unwrap(),
        ],
    );
    let pdf = if pdf.exists() { pdf } else { dir.join("notes.pdf") };

    let edges = all_edges();
    let cases: Vec<(PathBuf, &str)> = vec![
        (png.clone(), "jpg"),
        (png.clone(), "webp"),
        (png.clone(), "pdf"),
        (png.clone(), "tiff"),
        (wav.clone(), "mp3"),
        (wav.clone(), "flac"),
        (mp4.clone(), "gif"),
        (mp4.clone(), "mp3"), // video -> audio extract
        (docx.clone(), "pdf"),
        (pdf.clone(), "png"),  // pdf -> raster
        (pdf.clone(), "txt"),  // pdf -> text
        (md.clone(), "pdf"),   // multi-hop: md -> docx -> pdf
        (md.clone(), "png"),   // multi-hop x3: md -> docx -> pdf -> png
    ];

    println!("\n  flick-engine smoke test\n  {}", "-".repeat(66));
    let (mut pass, mut fail) = (0, 0);
    for (src, to) in &cases {
        let from = ext_of_path(&src.to_string_lossy());
        let route = plan_route(&from, to, &edges, 3);
        let route_str = match &route {
            Some(r) if r.is_empty() => "identity".to_string(),
            Some(r) => r
                .iter()
                .map(|s| format!("{:?}({}\u{2192}{})", s.engine, s.from, s.to))
                .collect::<Vec<_>>()
                .join(" + "),
            None => "NO ROUTE".to_string(),
        };
        let oc = convert_file(src, to, &out);
        let ok = oc.ok && !oc.outputs.is_empty() && oc.outputs.iter().all(|o| nonempty(o));
        println!(
            "  {} {:>4} \u{2192} {:<4}  [{}]",
            if ok { "PASS" } else { "FAIL" },
            from,
            to,
            route_str
        );
        if ok {
            pass += 1;
        } else {
            fail += 1;
            if let Some(e) = &oc.error {
                println!("         error: {e}");
            }
        }
    }

    // --- tools (Command wheel) ---
    println!("  {}", "-".repeat(66));
    let tool_cases: Vec<(PathBuf, &str)> = vec![
        (png.clone(), "Compress"),
        (png.clone(), "Resize"),
        (png.clone(), "Grayscale"),
        (pdf.clone(), "Compress"),
        (pdf.clone(), "First page \u{2192} PNG"),
        (mp4.clone(), "Mute"),
        (mp4.clone(), "Poster frame"),
    ];
    for (src, tool) in &tool_cases {
        let oc = run_tool(src, tool, &out);
        let ok = oc.ok && oc.outputs.iter().all(|o| nonempty(o));
        println!("  {} tool  {:<16} on {}", if ok { "PASS" } else { "FAIL" }, tool, src.file_name().unwrap().to_string_lossy());
        if ok { pass += 1; } else { fail += 1; if let Some(e) = &oc.error { println!("         error: {e}"); } }
    }

    // --- archive ---
    let paths = vec![png.to_string_lossy().into_owned(), wav.to_string_lossy().into_owned()];
    let oc = zip_together(&paths, &out, "zip");
    let zip_ok = oc.ok && oc.outputs.iter().all(|o| nonempty(o));
    println!("  {} archive  zip 2 files", if zip_ok { "PASS" } else { "FAIL" });
    if zip_ok { pass += 1; } else { fail += 1; }
    if let Some(z) = oc.outputs.first() {
        let oc2 = extract(z, &out);
        let ex_ok = oc2.ok && oc2.outputs.iter().all(|o| Path::new(o).exists());
        println!("  {} archive  extract", if ex_ok { "PASS" } else { "FAIL" });
        if ex_ok { pass += 1; } else { fail += 1; }
    }

    // --- detection + wheel planning ---
    println!("  {}", "-".repeat(66));
    let st = detect_engines();
    println!("  engines: {st:?}");
    let plan = plan_for_paths(&[png.to_string_lossy().into_owned()]);
    println!("  wheels for photo.png -> convert={:?} tools={:?} actions={:?}", plan.convert, plan.tools, plan.actions);

    println!("  {}\n  {} passed, {} failed\n", "-".repeat(66), pass, fail);
    std::process::exit(if fail == 0 { 0 } else { 1 });
}
