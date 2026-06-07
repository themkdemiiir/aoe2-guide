#!/usr/bin/env node

// scripts/sync-assets.mjs
// Fetches the img/ tree of SiegeEngineers/aoe2techtree at a pinned SHA
// and mirrors it into public/images/aoe2/.
//
// Refresh policy: bump the SHA constant in a deliberate PR.

import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { pipeline } from "node:stream/promises";

const REPO = "SiegeEngineers/aoe2techtree";
const SHA = process.env.AOE2TT_SHA || "b34082d13c31932d89788ad35af984896cbe050c";
const TARGET = path.resolve("public/images/aoe2");
const TMP = path.resolve(".cache/aoe2tt");

async function run() {
  await rm(TMP, { recursive: true, force: true });
  await mkdir(TMP, { recursive: true });

  const url = `https://codeload.github.com/${REPO}/tar.gz/${SHA}`;
  console.log("Fetching", url);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  const tarPath = path.join(TMP, "src.tar.gz");
  await pipeline(res.body, createWriteStream(tarPath));

  console.log("Extracting img/ subtree …");
  await new Promise((resolve, reject) => {
    const tar = spawn(
      "tar",
      ["xzf", tarPath, "-C", TMP, "--wildcards", "*/img/*", "--strip-components=2"],
      { stdio: "inherit" },
    );
    tar.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`tar exit ${code}`))));
  });

  // Remove the spurious nested img/ directory left by the wildcard extraction
  await rm(path.join(TMP, "img"), { recursive: true, force: true });
  // Also remove the downloaded tarball so it doesn't land in TARGET
  await rm(path.join(TMP, "src.tar.gz"), { force: true });

  console.log("Mirroring into", TARGET);
  await rm(TARGET, { recursive: true, force: true });
  await mkdir(TARGET, { recursive: true });
  // --strip-components=2 already placed img/ contents directly into TMP
  // rsync with trailing slash on source copies contents (not the directory itself)
  await new Promise((resolve, reject) => {
    const sync = spawn("rsync", ["-a", `${TMP}/`, `${TARGET}/`], { stdio: "inherit" });
    sync.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`rsync exit ${code}`))));
  });

  console.log("Done. Assets at", TARGET);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
