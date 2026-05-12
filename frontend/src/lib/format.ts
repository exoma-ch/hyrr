/**
 * Render-time formatters for decay modes, projectiles, and reaction notation.
 *
 * Wire format stays ASCII (e.g. "beta_plus", "alpha"); these helpers map onto
 * Greek symbols only at the display layer. CSV/parquet exports must keep the
 * raw identifier.
 */

const DECAY_MODE_MAP: Record<string, string> = {
  "beta_plus": "β⁺",
  "beta+": "β⁺",
  "b+": "β⁺",
  "positron": "β⁺",
  "ec/beta+": "EC/β⁺",
  "beta+/ec": "β⁺/EC",
  "beta_minus": "β⁻",
  "beta-": "β⁻",
  "b-": "β⁻",
  "β-": "β⁻",
  "β+": "β⁺",
  "electron": "β⁻",
  "alpha": "α",
  "a": "α",
  "gamma": "γ",
  "g": "γ",
  "ec": "EC",
  "kshellec": "EC",
  "lshellec": "EC",
  "mshellec": "EC",
  "k_shell_ec": "EC",
  "l_shell_ec": "EC",
  "m_shell_ec": "EC",
};

const PROJECTILE_MAP: Record<string, string> = {
  "alpha": "α",
  "a": "α",
  "gamma": "γ",
  "g": "γ",
  "he3": "³He",
  "he-3": "³He",
  "3he": "³He",
  "³he": "³He",
  "p": "p",
  "n": "n",
  "d": "d",
  "t": "t",
};

/** Map a decay-mode identifier to its render form (Greek where applicable). */
export function formatDecayMode(mode: string): string {
  if (!mode) return mode;
  const key = mode.trim().toLowerCase();
  return DECAY_MODE_MAP[key] ?? mode;
}

/** True if a raw decay-mode identifier is a shell-resolved electron-capture entry
 * (e.g. `KshellEC`, `LshellEC`, `MshellEC`, `K_shell_EC`). The K/L/M atomic-shell
 * distinction is physically meaningful, but for the decay-mode chip and decay-chain
 * presentation we collapse them all into a single `EC` bucket (see #198). */
export function isShellEC(mode: string): boolean {
  if (!mode) return false;
  const key = mode.trim().toLowerCase().replace(/_/g, "");
  return /^[klm]shellec$/.test(key);
}

export interface DecayChainEntry {
  mode: string;
  branching: number;
}

export interface DedupedDecayChainEntry<K> extends DecayChainEntry {
  key: K;
  /** All original mode/branching pairs that collapsed into this entry, in input order. */
  sources: DecayChainEntry[];
}

/**
 * Deduplicate decay-chain entries by a caller-supplied identity key — typically
 * `(Z, A, state)` for a parent or `(daughterZ, daughterA, daughterState)` for a
 * daughter. Mode strings inside each bucket are first aggregated via
 * {@link aggregateDecayModes} (so K/L/M shell-EC collapses into `EC`), then the
 * largest-branching surviving mode is chosen as the bucket's display mode.
 *
 * Branchings within a bucket are SUMMED (the bucket represents the total
 * probability of arriving at — or departing to — that nuclide regardless of
 * mode). Original entries are preserved on `sources[]` for tooltip rendering.
 */
export function dedupeDecayChain<T extends DecayChainEntry, K>(
  entries: readonly T[],
  keyOf: (entry: T) => K,
  keyToString: (key: K) => string = (k) => JSON.stringify(k),
): Array<DedupedDecayChainEntry<K> & Omit<T, keyof DecayChainEntry>> {
  type Bucket = DedupedDecayChainEntry<K> & Omit<T, keyof DecayChainEntry>;
  const buckets = new Map<string, Bucket>();
  for (const entry of entries) {
    const key = keyOf(entry);
    const ks = keyToString(key);
    const existing = buckets.get(ks);
    if (existing) {
      existing.sources.push({ mode: entry.mode, branching: entry.branching });
    } else {
      // Capture all fields from the original entry (including extras like name,
      // daughterZ, etc.) so consumers don't have to thread them through manually.
      const { mode: _mode, branching: _branching, ...rest } = entry as T & Record<string, unknown>;
      buckets.set(ks, {
        ...(rest as Omit<T, keyof DecayChainEntry>),
        key,
        mode: entry.mode,
        branching: 0,
        sources: [{ mode: entry.mode, branching: entry.branching }],
      } as Bucket);
    }
  }
  // Finalize each bucket: aggregate K/L/M shell-EC inside the bucket, pick the
  // dominant mode as the display label, and sum branchings.
  const result: Bucket[] = [];
  for (const bucket of buckets.values()) {
    const aggregated = aggregateDecayModes(bucket.sources);
    const dominant = aggregated.reduce((best, m) =>
      m.branching > best.branching ? m : best,
    );
    bucket.mode = dominant.mode;
    bucket.branching = aggregated.reduce((s, m) => s + m.branching, 0);
    result.push(bucket);
  }
  return result;
}

export interface AggregatedDecayMode {
  /** Canonical display key — `"ec"` for any K/L/M-shell EC, otherwise the original mode string. */
  mode: string;
  /** Summed branching ratio across all merged entries. */
  branching: number;
  /** Original wire identifiers that were folded into this bucket (in input order). */
  sources: string[];
}

/**
 * Aggregate a list of `{mode, branching}` entries so that K/L/M shell-resolved
 * electron-capture rows collapse into a single `"ec"` bucket whose branching is
 * the sum of its parts. All other modes pass through untouched (and keep their
 * original wire-form `mode` string). Order of first occurrence is preserved.
 *
 * The `sources` array on each output entry retains the original mode strings so
 * a tooltip can still surface the K/L/M breakdown if desired.
 */
export function aggregateDecayModes<T extends { mode: string; branching: number }>(
  modes: readonly T[],
): AggregatedDecayMode[] {
  const buckets = new Map<string, AggregatedDecayMode>();
  for (const m of modes) {
    const isEC = isShellEC(m.mode);
    const key = isEC ? "ec" : m.mode;
    const displayMode = isEC ? "ec" : m.mode;
    const existing = buckets.get(key);
    if (existing) {
      existing.branching += m.branching;
      existing.sources.push(m.mode);
    } else {
      buckets.set(key, {
        mode: displayMode,
        branching: m.branching,
        sources: [m.mode],
      });
    }
  }
  return Array.from(buckets.values());
}

/** Map a projectile/particle identifier to its render form. */
export function formatProjectile(p: string): string {
  if (!p) return p;
  const key = p.trim().toLowerCase();
  return PROJECTILE_MAP[key] ?? p;
}

/**
 * Replace particle codes inside a reaction notation string. Handles forms like
 * `(p,2n)`, `(α,γ)`, `(d,p)`, and the decay-arrow form `X →β+→ Y`. Idempotent
 * on already-Greek input.
 */
export function formatReaction(reaction: string): string {
  if (!reaction) return reaction;
  let out = reaction.replace(/\(([^()]*)\)/g, (_m, inner: string) => {
    const parts = inner.split(",").map((tok) => mapParticleToken(tok));
    return `(${parts.join(",")})`;
  });
  out = out.replace(/→\s*([A-Za-z+\-0-9³αβγ]+)\s*→/g, (_m, tok: string) => {
    return `→${mapParticleToken(tok)}→`;
  });
  return out;
}

/** Token mapper for reaction-internal particle codes (preserves multiplier prefix). */
function mapParticleToken(token: string): string {
  const trimmed = token.trim();
  const m = trimmed.match(/^(\d*)(.+)$/);
  if (!m) return trimmed;
  const [, prefix, rest] = m;
  const mapped = formatProjectile(rest) !== rest
    ? formatProjectile(rest)
    : formatDecayMode(rest) !== rest
      ? formatDecayMode(rest)
      : rest;
  return `${prefix}${mapped}`;
}

/**
 * Build a tooltip for a deduped decay-chain entry that summarizes the dominant
 * mode and its total branching, with the K/L/M shell breakdown (if any) and any
 * additional modes appended in brackets. Example output:
 *
 *   `EC (100.0%) [KshellEC 89.2%, LshellEC 9.3%, MshellEC 1.5%]`
 *   `β⁺ (97.1%), EC (2.9%)`
 */
export function formatDecayChainTooltip(
  sources: readonly DecayChainEntry[],
): string {
  if (sources.length === 0) return "";
  const aggregated = aggregateDecayModes(sources);
  const parts = aggregated.map((m) => {
    const label = `${formatDecayMode(m.mode)} (${(m.branching * 100).toFixed(1)}%)`;
    // Only annotate when more than one wire identifier folded into this bucket
    // (i.e. K/L/M shell-EC collapsed into "ec").
    if (m.sources.length <= 1) return label;
    const breakdown = sources
      .filter((s) => m.sources.includes(s.mode))
      .map((s) => `${s.mode} ${(s.branching * 100).toFixed(1)}%`)
      .join(", ");
    return `${label} [${breakdown}]`;
  });
  return parts.join(", ");
}

/**
 * Bidirectional alias map for filter matching. Maps Greek render forms back
 * to their ASCII identifiers and vice-versa so a user typing "beta+" still
 * matches a row whose displayed text is "β⁺".
 */
const FILTER_ALIASES: Array<[string, string]> = [
  ["β⁺", "beta+"],
  ["β⁻", "beta-"],
  ["α", "alpha"],
  ["γ", "gamma"],
  ["³he", "he3"],
];

/** Expand a haystack with both Greek and ASCII aliases for case-insensitive substring match. */
export function reactionFilterMatches(haystack: string, needle: string): boolean {
  if (!needle) return true;
  const hayLc = haystack.toLowerCase();
  const needLc = needle.trim().toLowerCase();
  if (hayLc.includes(needLc)) return true;
  for (const [greek, ascii] of FILTER_ALIASES) {
    if (needLc.includes(ascii) && hayLc.includes(greek)) return true;
    if (needLc.includes(greek) && hayLc.includes(ascii)) return true;
    const needSwapped = needLc.replace(ascii, greek);
    if (needSwapped !== needLc && hayLc.includes(needSwapped)) return true;
    const needSwappedRev = needLc.replace(greek, ascii);
    if (needSwappedRev !== needLc && hayLc.includes(needSwappedRev)) return true;
  }
  return false;
}
