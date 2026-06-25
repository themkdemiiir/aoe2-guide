# AOE2 Guide — next-session handoff (2026-06-25)

Project memory (`MEMORY.md` + files) auto-loads the deep context. This is the
task list + what's live + the active thread (replay parsing). Paste the prompt
below into a fresh session.

## What's LIVE on aoe2guide.com (shipped this session, all on `main`)

- **Homepage** redesigned as the *Illuminated Meta Codex* (`src/pages/[lang]/index.astro`).
- **+14 crawl-only ranked maps** (EM Runestones, Vulpine, Border Dispute, Karsts, …).
- **Combinable Map×Elo×Patch filters**, fully static (Cloudflare Pages, no server/DB):
  a precomputed cube `public/civ-cube.json` (~305KB gz) is fetched + filtered in
  the browser. Used by `/civs` (tier list), CivStats (per-civ slice), and map
  pages (patch). All have a server-rendered **no-JS fallback**.
- **Team ladder is current** (1.32M-match team RM crawl) and **matchups regenerated
  from the crawls** → **ONE source everywhere** ("self-collected World's Edge
  ranked ladder"); zero aoestats refs left in `src/data`. See memory:
  [[project_team_crawl]], [[project_homepage_codex]], [[project_deploy_pipeline]].
- Cloudflare auto-deploys on push to main (~2 min). GitHub CI is red only from
  pre-existing biome lint in `scripts/`/`tests/` — not a deploy blocker.

Site work is essentially DONE. Deferred (low priority): curated map prose (needs
sources), deeper matchups filtering (data non-combinable), 2500+ bucket is thin,
CI-lint cleanup. The 6-agent analysis of these is in the session transcript.

## The ACTIVE thread → replay parsing for full in-game events

The user wants to "time every important event" per game. Status (see
[[project_data_workbench]] for full detail):

- **Analysis workbench**: DuckDB `~/aoe2-guide/data-cache/aoe2.duckdb` on VM
  `mkd@192.168.111.7` — **aoestats archive (30.7M matches) + Relic crawls, deduped
  → 116M player-rows / 32.46M unique matches**, clean schema (`games` fact +
  `civs` dim + `civ_winrate`/`map_civ`/`civ_opening`/`civ_timing` summaries) +
  openings + age-up times. Web UI: tmux `duckui` → `start_ui_server()` on
  `127.0.0.1:4213`; reach via `ssh -L 4213:localhost:4213 -N -f mkd@192.168.111.7`.
- **Parser SOLVED**: **`aoe2rec`** (github aoe2ct/aoe2rec, Rust, MIT, `binrw`)
  parses the user's **current-patch** replays (v67) in ~ms with the **full action
  stream** (`world_time` + `game_command`) + civ per player. **Python mgz FAILS**
  on this patch. Built locally: `/tmp/aoe2rec` (`cargo build --release -p aoe2js`).
  Rust installed on the desktop via rustup.
- **PLAN**: DuckDB stores all (add an `events` table); aoe2rec parses; a **semantic
  extractor** maps `game_command`+`world_time` → age-ups / first-TC / military /
  build-order signature, keyed to `match_id`. Rewrite hot parts in Rust as needed.
- **Open question**: collecting *other* players' games needs the Relic
  **replay-download endpoint** (UNVERIFIED — replays expire in weeks, history is
  gone). The user's OWN + local replays parse now with no download
  (`~/.local/share/Steam/steamapps/compatdata/813780/pfx/.../Age of Empires 2 DE/<steamid>/savegame/*.aoe2record`).

## Next steps (the new session should do)

1. **Create a dedicated parse-worker VM** on the Proxmox host (steps given to the
   user; specs ~8 vCPU / 12 GB / 80 GB Ubuntu 24.04). Or reuse the existing VM.
2. Install Rust + DuckDB + clone aoe2rec on it.
3. **Build the PoC extractor**: aoe2rec (lib or `aoe2rec-py`) → extract events from
   local replays → write a DuckDB `events(match_id, profile_id, event, t_seconds)`
   table keyed to `games`. Prove the value chain on real local replays (no download).
4. Then decide collection-at-scale (verify the Relic replay-download endpoint).

## Hardware / gotchas

- Proxmox host: Ryzen 5 2600 (12T). Nodes seen: nvme **16C/62GB**, ssd **12C/15.5GB**.
  Compute is never the bottleneck (parse is ms/replay). A 2nd VM = isolation, not need.
- VM SSH: key must be loaded (`ssh-add -l`); push to GitHub needs it too.
- DuckDB is single-writer: query *through* the UI, or read source files in-memory.
- `pnpm build` runs the translation-audit + schema gate. Civ YAML is generated.

---

## PROMPT FOR THE NEW SESSION

> Continuing the AOE2 guide (memory has full context; read
> `docs/superpowers/NEXT-SESSION.md` first). The site is shipped + live. The active
> thread is **replay parsing**: `aoe2rec` (Rust) is validated — it parses my
> current-patch `.aoe2record` replays with the full event stream. Plan: store
> everything in the DuckDB workbench on the VM (add an `events` table), parse with
> aoe2rec, and write a semantic extractor (game_command+world_time → age-ups /
> build-order events). First: I'll create a parse-worker VM on Proxmox — help me
> set it up, then build the PoC extractor on my local replays (no download needed),
> writing events into DuckDB keyed to `games`. Then we'll look at collecting other
> players' replays. Keep it light — hobby project.
