# pipeline

A Rust workspace for the AOE2 guide's data pipeline. Crates are added one milestone at a time;
right now it holds only `crates/core` (`pipeline-core`), the typed, regex-free home for the
slug/elo-bucket/map/civ lookups that today live duplicated across JS
(`scripts/data-pipeline/lib/buckets.mjs`) and Rust (`replay-rs/src/analyze/{maps.rs,data.rs}`).
`core` stands alone for now — nothing else in the repo depends on it yet — and becomes the
shared data core that JS callers cut over to in a later milestone.
