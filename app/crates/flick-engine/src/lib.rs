//! flick-engine — the local file-conversion core for Flick.
//!
//! Given a set of dropped files it answers two questions:
//!   1. "What can I do with these?"  -> [`plan_for_paths`] builds the three wheels.
//!   2. "Do this."                   -> [`convert_file`] / [`run_tool`] run it.
//!
//! Every conversion is performed by an external CLI tool invoked as a separate
//! process, so this MIT crate never links any (possibly copyleft) converter.

pub mod detect;
pub mod exec;
pub mod format;
pub mod graph;
pub mod planner;

pub use detect::{detect_engines, EngineStatus};
pub use exec::{convert_file, extract, run_tool, zip_together, ConvertOutcome};
pub use format::{category_of, ext_of_path, popular_targets, tools_for, Category};
pub use graph::{all_edges, Edge, Engine};
pub use planner::{plan_route, reachable_targets, Route, Step};

use serde::Serialize;

/// Everything the UI needs to draw the three radial wheels for a drop.
#[derive(Serialize, Debug)]
pub struct DropPlan {
    pub from_ext: String,
    pub category: Category,
    pub count: usize,
    /// Shift wheel — formats to convert to.
    pub convert: Vec<String>,
    /// Command wheel — in-place tools.
    pub tools: Vec<String>,
    /// Control wheel — file actions (handled by the host app).
    pub actions: Vec<String>,
}

/// Inspect the dropped paths and decide what the three wheels should offer.
/// For a multi-file drop we key off the first file's type and add the
/// cross-file "Zip together" tool.
pub fn plan_for_paths(paths: &[String]) -> DropPlan {
    let first = paths.first().map(|p| ext_of_path(p)).unwrap_or_default();
    let cat = category_of(&first);
    let edges = all_edges();
    let reach = reachable_targets(&first, &edges, 3);

    let convert: Vec<String> = popular_targets(cat)
        .into_iter()
        .map(|s| s.to_string())
        .filter(|t| *t != first && reach.contains(t))
        .collect();

    let mut tools: Vec<String> = tools_for(cat).into_iter().map(|s| s.to_string()).collect();
    if paths.len() > 1 {
        tools.insert(0, "Zip together".to_string());
    }

    let mut actions: Vec<String> =
        vec!["Reveal in Finder".into(), "Copy path".into(), "Open".into(), "Duplicate".into()];
    if cat == Category::Archive {
        actions.insert(0, "Extract here".into());
    }

    DropPlan { from_ext: first, category: cat, count: paths.len(), convert, tools, actions }
}
