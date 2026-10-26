import { describe, expect, it } from 'vitest';

import { cargoVersion, findProblems, isSemver, setCargoVersion, stripTagPrefix } from './version.mjs';

const toml = `[workspace]\nmembers = ["a"]\n\n[workspace.package]\nversion = "1.2.3"\nedition = "2021"\n\n[profile.release]\nlto = true\n`;

describe('cargoVersion', () => {
  it('reads the workspace package version', () => {
    expect(cargoVersion(toml)).toBe('1.2.3');
  });
  it('ignores versions in other sections', () => {
    expect(cargoVersion(`[dependencies]\nfoo = { version = "9.9.9" }\n${toml}`)).toBe('1.2.3');
  });
  it('fails loudly when missing', () => {
    expect(() => cargoVersion('[workspace]\n')).toThrow(/no version/);
  });
});

describe('setCargoVersion', () => {
  it('rewrites only the workspace version', () => {
    const out = setCargoVersion(toml, '2.0.0');
    expect(cargoVersion(out)).toBe('2.0.0');
    expect(out).toContain('edition = "2021"');
  });
});

describe('semver and tags', () => {
  it('accepts release and pre-release versions', () => {
    for (const v of ['0.1.0', '1.10.2', '1.0.0-rc.1', '1.0.0+build.5']) expect(isSemver(v)).toBe(true);
  });
  it('rejects malformed versions', () => {
    for (const v of ['1.0', 'v1.0.0', '1.0.0.0', '', 'latest']) expect(isSemver(v)).toBe(false);
  });
  it('strips the v and refs/tags prefixes', () => {
    expect(stripTagPrefix('v1.2.3')).toBe('1.2.3');
    expect(stripTagPrefix('refs/tags/v1.2.3')).toBe('1.2.3');
    expect(stripTagPrefix('1.2.3')).toBe('1.2.3');
  });
});

describe('findProblems', () => {
  const ok = { 'Cargo.toml': '1.2.3', 'package.json': '1.2.3', 'tauri.conf.json': '1.2.3' };

  it('is happy when everything agrees', () => {
    expect(findProblems(ok)).toEqual([]);
    expect(findProblems(ok, 'v1.2.3')).toEqual([]);
  });
  it('reports disagreement between files', () => {
    expect(findProblems({ ...ok, 'package.json': '1.2.4' }).join()).toMatch(/disagree/);
  });
  it('reports a tag that does not match', () => {
    const problems = findProblems(ok, 'v1.3.0');
    expect(problems).toHaveLength(3);
    expect(problems[0]).toMatch(/tag is 1\.3\.0/);
  });
  it('reports invalid versions', () => {
    expect(findProblems({ a: 'banana' }).join()).toMatch(/not a valid semantic version/);
  });
});
