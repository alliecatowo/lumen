/**
 * Minimal SemVer 2.0.0 parsing and precedence, enough for registry ordering.
 * Build metadata is ignored for precedence, as the spec requires.
 */

export interface SemVer {
  major: number;
  minor: number;
  patch: number;
  prerelease: string[];
}

const SEMVER_RE =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*)?$/;

/** Parse a strict semver string, returning null when it is not valid. */
export function parseSemver(version: string): SemVer | null {
  if (typeof version !== 'string' || version.length > 64) return null;
  const m = SEMVER_RE.exec(version);
  if (!m) return null;
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    prerelease: m[4] ? m[4].split('.') : [],
  };
}

function cmpNum(a: number, b: number): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** Compare two parsed versions per SemVer 2.0.0 section 11. */
export function compareParsed(a: SemVer, b: SemVer): number {
  const core =
    cmpNum(a.major, b.major) || cmpNum(a.minor, b.minor) || cmpNum(a.patch, b.patch);
  if (core !== 0) return core;
  if (a.prerelease.length === 0 && b.prerelease.length === 0) return 0;
  if (a.prerelease.length === 0) return 1;
  if (b.prerelease.length === 0) return -1;
  const n = Math.max(a.prerelease.length, b.prerelease.length);
  for (let i = 0; i < n; i++) {
    const x = a.prerelease[i];
    const y = b.prerelease[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x);
    const yn = /^\d+$/.test(y);
    if (xn && yn) {
      const c = cmpNum(Number(x), Number(y));
      if (c !== 0) return c;
    } else if (xn) {
      return -1;
    } else if (yn) {
      return 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

/** Compare two version strings; invalid versions sort lowest, then lexically. */
export function compareVersions(a: string, b: string): number {
  const pa = parseSemver(a);
  const pb = parseSemver(b);
  if (pa && pb) return compareParsed(pa, pb);
  if (pa) return 1;
  if (pb) return -1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * Pick the `latest` version: the highest non-prerelease, or the highest
 * prerelease when the package only has prereleases.
 */
export function pickLatest(versions: string[]): string | null {
  if (versions.length === 0) return null;
  const sorted = [...versions].sort((a, b) => compareVersions(b, a));
  const stable = sorted.find((v) => {
    const p = parseSemver(v);
    return p !== null && p.prerelease.length === 0;
  });
  return stable ?? sorted[0];
}
