// Flick — Tauri host. Thin glue between the web UI and `flick-engine`.
// No conversion happens here; every engine call shells out to a CLI tool.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use flick_engine as engine;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Converted copies land next to the file they came from.
fn parent_of(p: &str) -> PathBuf {
    Path::new(p).parent().map(|d| d.to_path_buf()).unwrap_or_else(std::env::temp_dir)
}

fn ok_paths(paths: &[String]) -> engine::ConvertOutcome {
    engine::ConvertOutcome { ok: true, outputs: paths.to_vec(), error: None }
}

/// Bring the main window to the front (global hotkey / "Open with Flick").
fn summon(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn detect() -> engine::EngineStatus {
    engine::detect_engines()
}

#[tauri::command]
fn plan(paths: Vec<String>) -> engine::DropPlan {
    engine::plan_for_paths(&paths)
}

#[tauri::command]
fn convert(paths: Vec<String>, to: String) -> Vec<engine::ConvertOutcome> {
    paths.iter().map(|p| engine::convert_file(Path::new(p), &to, &parent_of(p))).collect()
}

#[tauri::command]
fn run_tool(paths: Vec<String>, tool: String) -> Vec<engine::ConvertOutcome> {
    if tool == "Zip together" {
        let out = paths.first().map(|p| parent_of(p)).unwrap_or_else(std::env::temp_dir);
        return vec![engine::zip_together(&paths, &out, "zip")];
    }
    paths.iter().map(|p| engine::run_tool(Path::new(p), &tool, &parent_of(p))).collect()
}

#[tauri::command]
fn action(paths: Vec<String>, action: String) -> engine::ConvertOutcome {
    match action.as_str() {
        "Reveal in Finder" => {
            for p in &paths {
                let _ = Command::new("open").arg("-R").arg(p).status();
            }
            ok_paths(&paths)
        }
        "Open" => {
            for p in &paths {
                let _ = Command::new("open").arg(p).status();
            }
            ok_paths(&paths)
        }
        // The webview puts the paths on the clipboard itself.
        "Copy path" => ok_paths(&paths),
        "Duplicate" => {
            let mut outs = Vec::new();
            for p in &paths {
                let src = Path::new(p);
                let dir = parent_of(p);
                let stem = src
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "copy".into());
                let dst = match src.extension().and_then(|e| e.to_str()) {
                    Some(ext) => dir.join(format!("{stem} copy.{ext}")),
                    None => dir.join(format!("{stem} copy")),
                };
                if std::fs::copy(src, &dst).is_ok() {
                    outs.push(dst.to_string_lossy().into_owned());
                }
            }
            engine::ConvertOutcome { ok: !outs.is_empty(), outputs: outs, error: None }
        }
        "Extract here" => {
            let mut outs = Vec::new();
            for p in &paths {
                outs.extend(engine::extract(p, &parent_of(p)).outputs);
            }
            engine::ConvertOutcome { ok: !outs.is_empty(), outputs: outs, error: None }
        }
        other => engine::ConvertOutcome {
            ok: false,
            outputs: Vec::new(),
            error: Some(format!("unknown action: {other}")),
        },
    }
}

fn main() {
    // Global "summon Flick" hotkey: Control + Option + Command + F.
    let hotkey = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER), Code::KeyF);

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        summon(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![detect, plan, convert, run_tool, action])
        .setup(move |app| {
            // Best-effort — never block launch if the hotkey is already taken.
            let _ = app.global_shortcut().register(hotkey);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Flick");

    // Files opened via Finder "Open With → Flick" arrive here.
    app.run(|app, event| {
        if let tauri::RunEvent::Opened { urls } = event {
            let paths: Vec<String> = urls
                .iter()
                .filter_map(|u| u.to_file_path().ok())
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if !paths.is_empty() {
                summon(app);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.emit("open-files", paths);
                }
            }
        }
    });
}
