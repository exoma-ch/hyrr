// Build-time provenance for the deployed bundle (#401).
//
// Emitted to `dist/version.json` so a deploy can be verified from the OUTSIDE —
// "is the thing I built actually the thing being served?". Until this existed,
// `scripts/deploy-eth.sh` rsynced and then echoed success without reading a
// single byte back, and three separate incidents (#529, #461, #565) were green
// pipelines over a wrong reality.
//
// Kept as a standalone module rather than inlined into vite.config.ts so the
// resolution rules below are unit-testable without running a full `vite build`.
import { execFileSync } from "node:child_process";

export interface VersionInfo {
  /** Bundle version — release-please keeps frontend/package.json in lockstep with the tag. */
  version: string;
  /** Full 40-char commit SHA, or "unknown" when it cannot be determined. */
  commit: string;
  /** ISO-8601 UTC build time. */
  built_at: string;
}

export interface VersionInfoDeps {
  env?: Record<string, string | undefined>;
  gitHead?: () => string | null;
  now?: () => Date;
}

const UNKNOWN_COMMIT = "unknown";

/**
 * Read HEAD from the surrounding work tree.
 *
 * Returns null rather than throwing on every failure mode — no git binary, no
 * repository (a tarball / nix sandbox build), a git that errors. Build
 * provenance is a nice-to-have; it must never be the reason a build fails.
 */
export function gitHeadFromWorkTree(): string | null {
  try {
    const head = execFileSync("git", ["rev-parse", "HEAD"], {
      encoding: "utf8",
      // stderr ignored: outside a repo git writes a fatal to it and we do not
      // want that noise in an otherwise clean build log.
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    return head === "" ? null : head;
  } catch {
    return null;
  }
}

/**
 * Commit precedence: explicit override → CI-provided SHA → local git → unknown.
 *
 * GITHUB_SHA is preferred over `git rev-parse HEAD` because on a `pull_request`
 * event the checked-out HEAD is a synthetic merge commit that exists nowhere in
 * the repository, which would make the recorded provenance unresolvable.
 */
export function resolveCommit(
  env: Record<string, string | undefined>,
  gitHead: () => string | null,
): string {
  const explicit = env.VITE_BUILD_COMMIT?.trim();
  if (explicit) return explicit;
  const ci = env.GITHUB_SHA?.trim();
  if (ci) return ci;
  return gitHead()?.trim() || UNKNOWN_COMMIT;
}

/**
 * Build timestamp — wall-clock, unless explicitly pinned.
 *
 * Deliberately does NOT honour SOURCE_DATE_EPOCH. Nix exports it globally with
 * a default of 315532800 (1980-01-01), so any build run from a nix shell or
 * devshell — which is every build on this project's dev host — would stamp
 * every deploy 1980-01-01. This field exists to answer "when did this bundle
 * actually get built", and a reproducibility convention that is on-by-default
 * in the ambient environment destroys precisely that. `dist/` is not
 * reproducibility-checked anywhere, so there is nothing to trade away.
 *
 * VITE_BUILD_TIMESTAMP stays available for a caller that genuinely wants to pin
 * it — opt-in, symmetric with VITE_BUILD_COMMIT, and impossible to set by
 * accident.
 */
export function resolveBuiltAt(
  env: Record<string, string | undefined>,
  now: () => Date,
): string {
  const pinned = env.VITE_BUILD_TIMESTAMP?.trim();
  if (pinned) {
    const date = new Date(pinned);
    if (!Number.isNaN(date.getTime())) return date.toISOString();
  }
  return now().toISOString();
}

export function buildVersionInfo(version: string, deps: VersionInfoDeps = {}): VersionInfo {
  const env = deps.env ?? process.env;
  return {
    version,
    commit: resolveCommit(env, deps.gitHead ?? gitHeadFromWorkTree),
    built_at: resolveBuiltAt(env, deps.now ?? (() => new Date())),
  };
}
