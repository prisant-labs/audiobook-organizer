#!/usr/bin/env node
/**
 * Fail if any tracked text file contains an em-dash (U+2014) or en-dash (U+2013).
 *
 * WHY THIS IS A SCRIPT, not an inline CI step. The first version was one line of
 * shell and had four separate ways to pass while catching nothing:
 *
 *   1. `grep -P '\xe2\x80\x94'` looks correct and is BLIND. It matched nothing at
 *      all, so the gate would have reported a clean tree forever. Caught only by
 *      asking whether it could find a PLANTED dash, not just whether the real tree
 *      came back clean.
 *   2. A trailing `|| true` turned a grep ERROR (bad locale, unreadable file) into
 *      a clean pass, which is the worst failure mode a gate can have.
 *   3. `grep -I` silently skipped whatever it guessed was binary, including UTF-16
 *      text.
 *   4. No `--` before filenames, so a file named like a flag parses as one.
 *
 * Decoding explicitly removes all four. A file that is not valid UTF-8 is
 * genuinely binary and cannot carry prose; everything else is decoded and scanned.
 * A file that cannot be READ is an error, never a pass.
 *
 * The two characters are built from code points and never written literally: a
 * literal one here would be an offender in the very file that bans them. The
 * project's PreToolUse hook enforces that, and it rejected two drafts of this file
 * for exactly that mistake.
 *
 * Run locally: node scripts/check-dashes.mjs
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const EM_DASH = String.fromCodePoint(0x2014);
const EN_DASH = String.fromCodePoint(0x2013);

/** Tracked files, NUL-separated so paths with spaces or newlines survive. */
function trackedFiles() {
  const out = execFileSync("git", ["ls-files", "-z"], { maxBuffer: 64 * 1024 * 1024 });
  return out.toString("utf8").split("\0").filter(Boolean);
}

const offenders = [];
const unreadable = [];
let scanned = 0;
let skippedBinary = 0;

for (const file of trackedFiles()) {
  let bytes;
  try {
    bytes = readFileSync(file);
  } catch (err) {
    // Never swallow this. An unreadable file is an unknown, and an unknown must
    // not be reported as a pass.
    unreadable.push(`${file}: ${err.message}`);
    continue;
  }

  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    // Not valid UTF-8, so genuinely binary: prose cannot hide here.
    skippedBinary += 1;
    continue;
  }

  scanned += 1;
  text.split(/\r?\n/).forEach((line, i) => {
    const hasEm = line.includes(EM_DASH);
    if (hasEm || line.includes(EN_DASH)) {
      const which = hasEm ? "em-dash U+2014" : "en-dash U+2013";
      offenders.push(`${file}:${i + 1}  (${which})  ${line.trim().slice(0, 100)}`);
    }
  });
}

if (unreadable.length > 0) {
  console.error("::error::the dash check could not read some tracked files, so it cannot pass");
  unreadable.forEach((u) => console.error(`  ${u}`));
  process.exit(1);
}

if (offenders.length > 0) {
  console.error(
    "::error::em-dash (U+2014) or en-dash (U+2013) found. Use ' - ', a comma, a colon, " +
      "or a sentence break; numeric ranges use a plain hyphen.",
  );
  offenders.forEach((o) => console.error(`  ${o}`));
  process.exit(1);
}

console.log(
  `OK: no em-dashes or en-dashes in ${scanned} tracked text files ` +
    `(${skippedBinary} binary files skipped).`,
);
