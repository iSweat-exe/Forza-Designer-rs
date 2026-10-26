import { describe, expect, it } from 'vitest';

import { baseName, clamp, estimateRemaining, formatDuration, isImagePathLike } from './lib';

describe('formatDuration', () => {
  it('formats minutes and hours', () => {
    expect(formatDuration(5)).toBe('0:05');
    expect(formatDuration(75)).toBe('1:15');
    expect(formatDuration(3700)).toBe('1:01:40');
  });
  it('never goes negative', () => {
    expect(formatDuration(-3)).toBe('0:00');
  });
});

describe('estimateRemaining', () => {
  it('waits for enough samples', () => {
    expect(estimateRemaining(2, 100, 10)).toBeNull();
  });
  it('extrapolates the pace so far', () => {
    expect(estimateRemaining(50, 100, 100)).toBe(100);
  });
  it('is null once finished', () => {
    expect(estimateRemaining(100, 100, 100)).toBeNull();
  });
});

describe('paths', () => {
  it('baseName handles both separators', () => {
    expect(baseName('C:\\a\\b\\pic.png')).toBe('pic.png');
    expect(baseName('/a/b/pic.png')).toBe('pic.png');
  });
  it('recognises image extensions case-insensitively', () => {
    expect(isImagePathLike('x.PNG')).toBe(true);
    expect(isImagePathLike('x.json')).toBe(false);
  });
  it('clamp bounds a value', () => {
    expect(clamp(5, 1, 3)).toBe(3);
    expect(clamp(-1, 1, 3)).toBe(1);
  });
});
