import { describe, expect, it } from 'vitest';
import { compareVersions, parseSemver, pickLatest } from '../src/semver';

describe('semver', () => {
  it('orders prereleases below releases and numerically', () => {
    const sorted = ['1.0.0', '1.0.0-rc.1', '1.0.0-alpha', '1.0.0-alpha.2', '1.0.0-alpha.10', '0.9.9', '1.10.0', '1.2.0']
      .sort(compareVersions);
    expect(sorted).toEqual(['0.9.9', '1.0.0-alpha', '1.0.0-alpha.2', '1.0.0-alpha.10', '1.0.0-rc.1', '1.0.0', '1.2.0', '1.10.0']);
  });
  it('rejects invalid versions', () => {
    for (const v of ['1', '1.0', '01.0.0', 'a.b.c', '1.0.0-', '1.0.0..1', '']) expect(parseSemver(v)).toBeNull();
  });
  it('latest ignores prereleases when a stable exists', () => {
    expect(pickLatest(['1.0.0', '1.1.0-rc.1'])).toBe('1.0.0');
    expect(pickLatest(['1.0.0-a', '1.0.0-b'])).toBe('1.0.0-b');
    expect(pickLatest([])).toBeNull();
  });
});
