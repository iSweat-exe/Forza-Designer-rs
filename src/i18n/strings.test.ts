import { describe, expect, it } from 'vitest';

import { detectLang, dictionaries, format, type StringKey } from './strings';

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe('dictionaries', () => {
  const keys = Object.keys(dictionaries.en) as StringKey[];

  it('French has exactly the English keys', () => {
    expect(Object.keys(dictionaries.fr).sort()).toEqual([...keys].sort());
  });

  it.each(keys)('%s uses the same placeholders in both languages', (key) => {
    expect(placeholders(dictionaries.fr[key])).toEqual(placeholders(dictionaries.en[key]));
  });

  it('has no empty strings', () => {
    for (const lang of Object.values(dictionaries)) {
      for (const v of Object.values(lang)) expect(v.trim()).not.toBe('');
    }
  });
});

describe('format', () => {
  it('substitutes variables', () => {
    expect(format('{n} of {total}', { n: 1, total: 5 })).toBe('1 of 5');
  });
  it('leaves unknown placeholders visible instead of printing undefined', () => {
    expect(format('{a} {b}', { a: 'x' })).toBe('x {b}');
  });
  it('returns the template when there are no variables', () => {
    expect(format('plain')).toBe('plain');
  });
});

describe('detectLang', () => {
  it('maps French locales to fr and everything else to en', () => {
    const set = (lang: string) => Object.defineProperty(navigator, 'language', { value: lang, configurable: true });
    set('fr-CA');
    expect(detectLang()).toBe('fr');
    set('de-DE');
    expect(detectLang()).toBe('en');
  });
});
