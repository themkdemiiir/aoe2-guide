#!/usr/bin/env bash
cd "$(cd "$(dirname "$0")/../.." && pwd)"
node scripts/data-pipeline/collect-relic.mjs --players "${1:-50315}" > /tmp/gapcrawl.log 2>&1
echo DONE > /tmp/gapcrawl.done
