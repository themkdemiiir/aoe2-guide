//! Thin CLI over the `refdata` library: writes `unit-stats.json`, `game-facts.json`,
//! `unit-names.json`, `tech-names.json`, and `icon-map.json` — all derived from the committed
//! aoe2techtree source slices baked into this binary at compile time (see `lib.rs`'s doc). Every
//! output but `icon-map.json` needs no other input, so there is no source-path flag to get wrong;
//! `icon-map.json` additionally crosses the committed slice against a live scan of
//! `public/images/aoe2/{Unit,Tech,Building,Civs}/` (thousands of binary PNGs, which can't be
//! `include_str!`'d) — that scan is this binary's one genuine filesystem READ, kept here in the
//! imperative shell per this crate's functional-core/imperative-shell split (see
//! `icon_map`'s doc); its location is computed, not a flag (see
//! `refdata::icon_map::default_assets_dir`'s doc).
//!
//! Unlike every other pipeline binary, this one touches no database and needs no `DATABASE_URL` —
//! it's a pure local-file transform, so `main` is synchronous (no `#[tokio::main]`) and only pulls
//! in `pipeline_core::cli::init_tracing` from the shared bootstrap.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use pipeline_core::cli::init_tracing;
use refdata::icon_map::AssetInventory;
use serde::Serialize;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Parser)]
#[command(
    name = "refdata",
    version,
    about = "Derive src/data/{unit-stats,game-facts,unit-names,tech-names,icon-map}.json from the committed aoe2techtree reference-data slices"
)]
struct Cli {
    /// Directory to write the five output files into (created if missing). NEVER `src/data` for
    /// this task — the Astro site's committed files stay untouched until an explicit cutover.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

fn main() {
    init_tracing(DEFAULT_LOG_FILTER);
    let cli = Cli::parse();

    if let Err(err) = run(&cli.out) {
        tracing::error!(error = %format!("{err:#}"), "refdata generation failed");
        std::process::exit(1);
    }
}

fn run(out_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create output directory {}", out_dir.display()))?;

    let unit_stats = refdata::unit_stats::build_from_committed_reference_data()
        .context("building unit-stats.json")?;
    write_json(&out_dir.join("unit-stats.json"), &unit_stats)?;

    let game_facts = refdata::game_facts::build_from_committed_reference_data()
        .context("building game-facts.json")?;
    write_json(&out_dir.join("game-facts.json"), &game_facts)?;

    let unit_names =
        refdata::unit_tech_names::build_unit_names().context("building unit-names.json")?;
    write_json(&out_dir.join("unit-names.json"), &unit_names)?;

    let tech_names =
        refdata::unit_tech_names::build_tech_names().context("building tech-names.json")?;
    write_json(&out_dir.join("tech-names.json"), &tech_names)?;

    let assets_dir = refdata::icon_map::default_assets_dir();
    let inventory = scan_asset_inventory(&assets_dir)
        .with_context(|| format!("scanning {}", assets_dir.display()))?;
    let icon_map = refdata::icon_map::build(&inventory).context("building icon-map.json")?;
    write_json(&out_dir.join("icon-map.json"), &icon_map)?;

    tracing::info!(
        units = unit_stats.units.len(),
        facts = game_facts.units.len(),
        unit_names = unit_names.map.len(),
        tech_names = tech_names.map.len(),
        icon_units = icon_map.units.len(),
        icon_techs = icon_map.techs.len(),
        icon_buildings = icon_map.buildings.len(),
        icon_civs = icon_map.civs.len(),
        out = %out_dir.display(),
        "wrote unit-stats.json, game-facts.json, unit-names.json, tech-names.json, icon-map.json"
    );
    Ok(())
}

/// Scans `assets_dir`'s `{Unit,Tech,Building,Civs}/` subdirectories into an
/// [`AssetInventory`] — the one filesystem READ this binary performs outside its `include_str!`'d
/// committed slices (see the module doc). `Unit`/`Tech`/`Building` become picture-index id sets
/// (a PNG's filename stem); `Civs` becomes the FINAL slug -> path map directly (civ resolution is
/// filename-only, matching `scripts/build-icon-map.mjs` — see `icon_map`'s doc).
fn scan_asset_inventory(assets_dir: &Path) -> anyhow::Result<AssetInventory> {
    let png_ids = |sub: &str| -> anyhow::Result<HashSet<String>> {
        let dir = assets_dir.join(sub);
        let ids = fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".png").map(str::to_string))
            .collect();
        Ok(ids)
    };

    let civs_dir = assets_dir.join("Civs");
    let civs: BTreeMap<String, String> = fs::read_dir(&civs_dir)
        .with_context(|| format!("reading {}", civs_dir.display()))?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".png"))
        .map(|name| {
            let slug = name.trim_end_matches(".png").to_lowercase();
            let path = format!("/images/aoe2/Civs/{name}");
            (slug, path)
        })
        .collect();

    Ok(AssetInventory {
        unit_ids: png_ids("Unit")?,
        tech_ids: png_ids("Tech")?,
        building_ids: png_ids("Building")?,
        civs,
    })
}

/// Pretty-prints `doc` with a trailing newline, matching the JS generators'
/// `` `${JSON.stringify(output, null, 2)}\n` `` exactly (serde_json's default `PrettyFormatter`
/// also uses 2-space indentation).
fn write_json<T: Serialize>(path: &Path, doc: &T) -> anyhow::Result<()> {
    let mut json = serde_json::to_string_pretty(doc)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
