/**
 * Parser for `scripts/frontend-data-libraries.txt` — the SSoT list of
 * nuclear-data libraries shipped in the frontend bundle (#651, epic #649).
 *
 * There are two readers of that file: `scripts/copy-frontend-data.sh` (bash,
 * writes the bundle) and the `hyrr-data-manifest` vite plugin (this side,
 * asserts the bundle matches). They MUST agree — a disagreement means the build
 * either rejects a correct bundle or accepts an incomplete one.
 *
 * They did not agree, and it cost a Windows release (#677). The plugin used to
 * strip comments inline with `line.replace(/#.*$/, "")`. In a JS regex `.` does
 * not match `\r`, so on a CRLF checkout — which is what GitHub's Windows
 * runners produce, since the repo had no `.gitattributes` — `#.*$` never
 * matched and every comment line survived as a "library". The three comment
 * lines that happen to contain a colon parsed as `name:subdir` specs, so the
 * build demanded subdirectories named after prose:
 *
 *     missing subdir:  one entry per line, `name` (→ xs/) or `name/
 *
 * The bundle was complete; the parser was wrong. The Windows desktop job died
 * on it, v0.21.0 shipped with no `.exe`/`.msi` and no `windows-x86_64` entry in
 * `latest.json`, and the diagnosis pointed at the data copy instead.
 *
 * Hence two rules here, both load-bearing:
 *   1. Normalize CRLF/CR/LF before doing anything else.
 *   2. Validate every surviving line against a strict pattern and throw with a
 *      line number. A parse that goes wrong must say so as a parse error — not
 *      launder itself into a plausible-looking data error somewhere downstream.
 */

import { readFileSync } from "node:fs";

export interface LibrarySpec {
  /** The entry verbatim, e.g. `endfb-8.0:neutron-xs`. */
  spec: string;
  /** Catalog name, e.g. `endfb-8.0`. */
  library: string;
  /** Destination directory under the bundle root; defaults to `xs`. */
  subdir: string;
  /** 1-based line number in the source file, for error messages. */
  line: number;
}

/**
 * `name` or `name:subdir`. Deliberately narrow: these become path segments
 * under `frontend/public/data/parquet`, and anything outside this alphabet is
 * far more likely to be a parse gone wrong than a library someone meant.
 */
const ENTRY = /^([A-Za-z0-9][A-Za-z0-9._-]*)(?::([A-Za-z0-9][A-Za-z0-9._-]*))?$/;

/** Default destination for a bare `name` entry — mirrors copy-frontend-data.sh. */
export const DEFAULT_SUBDIR = "xs";

export function parseLibraryList(
  text: string,
  source = "scripts/frontend-data-libraries.txt",
): LibrarySpec[] {
  const specs: LibrarySpec[] = [];

  // A UTF-8 BOM is stripped explicitly rather than left to `.trim()`. JS treats
  // U+FEFF as whitespace so trim() would eat it silently, but POSIX
  // `[[:space:]]` does not — so the bash twin would reject a BOM'd first entry
  // that this side quietly accepted. That is precisely the #677 shape: two
  // parsers disagreeing over a byte you cannot see. Handle it deliberately, on
  // both sides, and it cannot drift.
  const body = text.replace(/^﻿/, "");

  // Split on LF and CRLF, and NOT on a lone CR. Git does not produce CR-only
  // files, and `read` in the bash twin cannot split on them either — so
  // accepting them here would be lenience the other parser doesn't share. An
  // interior CR then survives into the entry and fails validation on both
  // sides, which is the outcome we want: loud, and identical in both languages.
  const lines = body.split(/\r?\n/);

  for (let i = 0; i < lines.length; i++) {
    // Comments run to end of line. Split on the literal `#` rather than a
    // regex with `$` — no anchor, so no line-terminator subtleties to get
    // wrong a second time.
    const entry = lines[i].split("#")[0].trim();
    if (!entry) continue;

    const m = ENTRY.exec(entry);
    if (!m) {
      throw new Error(
        `${source}:${i + 1}: malformed entry ${JSON.stringify(entry)}.\n` +
          "Expected `name` or `name:subdir` (letters, digits, dot, dash, underscore).\n" +
          "If this looks like prose, the comment stripping is broken — fix the parser, " +
          "not the list.",
      );
    }

    specs.push({
      spec: entry,
      library: m[1],
      subdir: m[2] ?? DEFAULT_SUBDIR,
      line: i + 1,
    });
  }

  if (specs.length === 0) {
    throw new Error(`${source}: no libraries listed — the frontend would ship no cross-sections.`);
  }

  return specs;
}

export function readLibraryList(path: string): LibrarySpec[] {
  return parseLibraryList(readFileSync(path, "utf8"), path);
}
