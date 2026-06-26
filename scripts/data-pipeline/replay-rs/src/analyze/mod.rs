//! Replay -> per-player macro-coaching report (the `analyze` subcommand).
//!
//! Pipeline: parse (aoe2rec) -> walk (events+meta+elo) -> metrics -> position ->
//! benchmark compare -> report. Pure metric logic (`metrics`) is isolated from the
//! parser bridge (`walk`) so it unit-tests with hand-built event vectors.

pub mod model;
pub mod maps;
pub mod metrics;
pub mod walk;
pub mod data;
pub mod position;
pub mod float;
pub mod compare;
pub mod report;

pub use model::{AnalyzeArgs, Input};

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use aoe2rec::Savegame;

use crate::api;

pub fn run(args: AnalyzeArgs) -> Result<()> {
    let game = load_game(&args.input)?;
    let w = walk::walk(&game);

    let map_table = maps::load();
    let (map_name, family) = map_table.lookup(w.meta.map_id);
    let map_name = map_name.to_string();
    let map_slug = maps::slug(&map_name); // benchmark key (e.g. "Black Forest" -> "blackforest")

    let roles = position::assign_roles(&w.players, family);
    let bases: HashMap<i32, (f32, f32)> = w
        .players
        .iter()
        .filter_map(|p| {
            metrics::base_centroid(&w.evs, p.player_number, 360_000).map(|b| (p.player_number, b))
        })
        .collect();
    let coords = position::coord_metrics(&w.evs, &w.players, &bases);

    let costs = data::load_costs();
    let bench = data::load_benchmark();
    let civs = data::load_civs();

    let player_metrics = compare::build_metrics(&w, &costs, &roles, &coords);
    let findings = compare::findings(&player_metrics, &bench, &civs, family, &map_slug);

    let you = resolve_you(&args.you, &w);
    print!("{}", report::render(&w.meta, &map_name, family, &player_metrics, &findings, you));
    Ok(())
}

fn load_game(input: &Input) -> Result<Savegame> {
    match input {
        Input::File(p) => {
            Savegame::from_file(p).map_err(|e| anyhow!("parse {}: {e}", p.display()))
        }
        Input::MatchId(id) => {
            let client = api::build_client()?;
            let per = api::get_replay_files(&client, &[*id])?;
            let files = per.get(id).ok_or_else(|| {
                anyhow!("match {id}: expired or not found (replays age out after ~weeks)")
            })?;
            let best = api::best_file(files).ok_or_else(|| {
                anyhow!("match {id}: no uploaded replay (all players' files missing)")
            })?;
            let url = best.url.clone().ok_or_else(|| anyhow!("match {id}: replay has no url"))?;
            let bytes = api::download_replay(&client, &url)?;
            Savegame::from_bytes(bytes).map_err(|e| anyhow!("parse match {id}: {e}"))
        }
    }
}

/// "you" = the `--you NAME` match (case-insensitive), else the recording player.
fn resolve_you(you: &Option<String>, w: &walk::Walked) -> i32 {
    if let Some(name) = you {
        if let Some(p) = w.players.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            return p.player_number;
        }
    }
    w.meta.rec_player
}
