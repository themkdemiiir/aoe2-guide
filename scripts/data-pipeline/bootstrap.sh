#!/usr/bin/env bash
# One-shot: install deps, collect ranked matches, aggregate -> src/data/civ-meta.json.
# Run on the VM:  bash ~/aoe2-guide/scripts/data-pipeline/bootstrap.sh
# Env knobs:      PLAYERS=3000  MIN_GAMES=200
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "== deps =="
sudo apt-get update -qq
sudo apt-get install -y -qq curl ca-certificates git >/dev/null
if ! command -v node >/dev/null; then
  echo "installing Node 20…"
  curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - >/dev/null
  sudo apt-get install -y -qq nodejs >/dev/null
fi
command -v pnpm >/dev/null || sudo npm i -g pnpm >/dev/null
[ -d node_modules ] || pnpm install --silent

echo "== collecting ranked 1v1 RM matches (slow part)… =="
node scripts/data-pipeline/collect-relic.mjs --players "${PLAYERS:-3000}"

echo "== aggregating -> src/data/civ-meta.json =="
node scripts/data-pipeline/aggregate-civmeta.mjs --min-games "${MIN_GAMES:-200}"

echo "✅ done — src/data/civ-meta.json is ready on this box."
