#!/usr/bin/env node
// Single source of truth for the Audiobook Organizer version bump.
//
// Adapted from the reference project's scripts/bump-version.mjs
// (E:/Projects/product-on-purpose/repo-sync-tool/scripts/bump-version.mjs),
// with a --check self-test mode added (v0.1.0 spine, Phase 1 brief item 6).
//
// The version is carried in four files that nothing otherwise keeps in
// sync. This script is the only sanctioned way to change it: it rewrites all
// four atomically so a release can never ship a bundle whose installer
// version disagrees with the crate version.
//
//   1. Cargo.toml                 [workspace.package] version (inherited by abo-core)
//   2. src-tauri/Cargo.toml       [package] version             (the desktop binary crate)
//   3. package.json               top-level "version"           (frontend package)
//   4. src-tauri/tauri.conf.json  "version"                     (stamps the MSI/NSIS/.app/.dmg)
//
// Usage:
//   node scripts/bump-version.mjs <semver>   e.g. node scripts/bump-version.mjs 0.2.0
//   node scripts/bump-version.mjs --check    verify all four files already agree; exits
//                                             0 if consistent, 1 otherwise. Makes no changes.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const arg = process.argv[2];
const root = join(dirname(fileURLToPath(import.meta.url)), '..');

const SOURCES = [
  { relPath: 'Cargo.toml', kind: 'toml', section: 'workspace.package' },
  { relPath: 'src-tauri/Cargo.toml', kind: 'toml', section: 'package' },
  { relPath: 'package.json', kind: 'json' },
  { relPath: 'src-tauri/tauri.conf.json', kind: 'json' },
];

function readTomlSectionVersion(relPath, section) {
  const path = join(root, relPath);
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  let inSection = false;
  for (const line of lines) {
    const header = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (header) {
      inSection = header[1] === section;
      continue;
    }
    if (inSection) {
      const m = line.match(/^\s*version\s*=\s*"([^"]*)"/);
      if (m) return m[1];
    }
  }
  return null;
}

function readJsonVersion(relPath) {
  const path = join(root, relPath);
  const text = readFileSync(path, 'utf8');
  const m = text.match(/"version"\s*:\s*"([^"]*)"/);
  return m ? m[1] : null;
}

function readVersion(source) {
  return source.kind === 'toml'
    ? readTomlSectionVersion(source.relPath, source.section)
    : readJsonVersion(source.relPath);
}

// --check: read all four files, report their versions, and exit non-zero if
// they disagree or any is missing. No file is written.
function runCheck() {
  console.log('Checking version agreement across the four pinned files:');
  const results = SOURCES.map((s) => ({ ...s, version: readVersion(s) }));
  let ok = true;
  for (const r of results) {
    const label = r.section ? `${r.relPath} [${r.section}]` : r.relPath;
    if (r.version === null) {
      console.error(`  MISSING  ${label}: no version key found`);
      ok = false;
    } else {
      console.log(`  ${r.version.padEnd(12)} ${label}`);
    }
  }
  const versions = new Set(results.map((r) => r.version).filter(Boolean));
  if (versions.size > 1) {
    console.error(`Version mismatch: found ${versions.size} distinct values: ${[...versions].join(', ')}`);
    ok = false;
  }
  if (!ok) {
    console.error('FAIL: versions are not in agreement.');
    process.exit(1);
  }
  console.log(`OK: all four files agree on version ${[...versions][0]}.`);
}

// Replace the first `version = "..."` line that sits inside the named
// [section]. Section-aware so we never touch the version pins in
// [workspace.dependencies].
function bumpTomlSection(relPath, section, newVersion) {
  const path = join(root, relPath);
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  let inSection = false;
  let done = false;
  for (let i = 0; i < lines.length; i++) {
    const header = lines[i].match(/^\s*\[([^\]]+)\]\s*$/);
    if (header) {
      inSection = header[1] === section;
      continue;
    }
    if (inSection && !done && /^\s*version\s*=\s*"/.test(lines[i])) {
      lines[i] = lines[i].replace(/version\s*=\s*"[^"]*"/, `version = "${newVersion}"`);
      done = true;
    }
  }
  if (!done) throw new Error(`No version line found in [${section}] of ${relPath}`);
  writeFileSync(path, lines.join('\n'));
  console.log(`  ${relPath} [${section}] -> ${newVersion}`);
}

// Replace the first top-level "version": "..." in a JSON file (preserves
// formatting).
function bumpJsonVersion(relPath, newVersion) {
  const path = join(root, relPath);
  const text = readFileSync(path, 'utf8');
  const re = /("version"\s*:\s*)"[^"]*"/;
  if (!re.test(text)) throw new Error(`No "version" key found in ${relPath}`);
  writeFileSync(path, text.replace(re, `$1"${newVersion}"`));
  console.log(`  ${relPath} version -> ${newVersion}`);
}

function runBump(newVersion) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(newVersion)) {
    console.error('Usage: node scripts/bump-version.mjs <semver>   (e.g. 0.2.0 or 0.2.0-beta.1)');
    console.error('       node scripts/bump-version.mjs --check    (verify agreement, no changes)');
    process.exit(1);
  }
  console.log(`Bumping Audiobook Organizer to ${newVersion}:`);
  bumpTomlSection('Cargo.toml', 'workspace.package', newVersion);
  bumpTomlSection('src-tauri/Cargo.toml', 'package', newVersion);
  bumpJsonVersion('package.json', newVersion);
  bumpJsonVersion('src-tauri/tauri.conf.json', newVersion);
  console.log('Done. Review: git diff -- Cargo.toml src-tauri/Cargo.toml package.json src-tauri/tauri.conf.json');
  console.log('Then run `cargo check` and `pnpm install` so the lockfiles pick up the new version.');
}

if (arg === '--check') {
  runCheck();
} else {
  runBump(arg);
}
