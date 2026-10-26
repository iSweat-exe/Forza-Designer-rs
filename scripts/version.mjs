#!/usr/bin/env node
// Keeps the version in Cargo.toml, package.json and tauri.conf.json in sync and,
// in release builds, equal to the git tag.
//
//   node scripts/version.mjs            check that all three agree
//   node scripts/version.mjs v1.2.3     ...and that they equal the tag
//   node scripts/version.mjs --set 1.2.3  write the version everywhere

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = new URL('../', import.meta.url);
const read = (p) => readFileSync(new URL(p, root), 'utf8');

/** `version = "x"` inside `[workspace.package]`. */
export function cargoVersion(toml) {
  const section = toml.match(/\[workspace\.package\]([\s\S]*?)(?:\n\[|$)/);
  const m = section?.[1]?.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error('no version in [workspace.package] of Cargo.toml');
  return m[1];
}

export function setCargoVersion(toml, version) {
  return toml.replace(/(\[workspace\.package\][\s\S]*?\n\s*version\s*=\s*")[^"]+(")/, `$1${version}$2`);
}

export function stripTagPrefix(tag) {
  return tag.replace(/^refs\/tags\//, '').replace(/^v/, '');
}

export function isSemver(v) {
  return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(v);
}

/** Returns a list of problems (empty when everything is consistent). */
export function findProblems(versions, tag) {
  const problems = [];
  for (const [file, v] of Object.entries(versions)) {
    if (!isSemver(v)) problems.push(`${file}: "${v}" is not a valid semantic version`);
  }
  const distinct = new Set(Object.values(versions));
  if (distinct.size > 1) {
    problems.push(
      `versions disagree: ${Object.entries(versions)
        .map(([f, v]) => `${f}=${v}`)
        .join(', ')}`,
    );
  }
  if (tag !== undefined) {
    const expected = stripTagPrefix(tag);
    for (const [file, v] of Object.entries(versions)) {
      if (v !== expected) problems.push(`${file} is ${v} but the tag is ${expected}`);
    }
  }
  return problems;
}

function current() {
  return {
    'Cargo.toml': cargoVersion(read('Cargo.toml')),
    'package.json': JSON.parse(read('package.json')).version,
    'src-tauri/tauri.conf.json': JSON.parse(read('src-tauri/tauri.conf.json')).version,
  };
}

function main(argv) {
  if (argv[0] === '--set') {
    const v = argv[1];
    if (!v || !isSemver(v)) throw new Error(`usage: version.mjs --set <semver> (got ${JSON.stringify(v)})`);
    writeFileSync(new URL('Cargo.toml', root), setCargoVersion(read('Cargo.toml'), v));
    for (const file of ['package.json', 'src-tauri/tauri.conf.json']) {
      const json = JSON.parse(read(file));
      json.version = v;
      writeFileSync(new URL(file, root), `${JSON.stringify(json, null, 2)}\n`);
    }
    console.log(`version set to ${v} (run \`cargo update -w\` to refresh Cargo.lock)`);
    return 0;
  }
  const versions = current();
  const problems = findProblems(versions, argv[0]);
  if (problems.length > 0) {
    for (const p of problems) console.error(`error: ${p}`);
    return 1;
  }
  console.log(`version ${Object.values(versions)[0]} is consistent`);
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(2);
  }
}
