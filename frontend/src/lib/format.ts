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
