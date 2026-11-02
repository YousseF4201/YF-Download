import assert from 'node:assert';
import test, { describe } from 'node:test';
import {
  calculateEta,
  calculateProgress,
  calculateSpeed,
  formatBytes,
  formatEta,
  formatSpeed,
  getFileCategory,
} from '../src/lib/utils/formatters.ts';

describe('formatBytes', () => {
  test('handles zero and negative or null/undefined', () => {
    assert.strictEqual(formatBytes(0), '0 B');
    assert.strictEqual(formatBytes(-100), '0 B');
    assert.strictEqual(formatBytes(null), '0 B');
    assert.strictEqual(formatBytes(undefined), '0 B');
  });

  test('formats byte sizes accurately', () => {
    assert.strictEqual(formatBytes(500), '500 B');
    assert.strictEqual(formatBytes(1024), '1.00 KB');
    assert.strictEqual(formatBytes(1536), '1.50 KB');
    assert.strictEqual(formatBytes(1048576), '1.00 MB');
    assert.strictEqual(formatBytes(1073741824), '1.00 GB');
  });
});

describe('formatSpeed', () => {
  test('formats speed values', () => {
    assert.strictEqual(formatSpeed(0), '0 B/s');
    assert.strictEqual(formatSpeed(null), '0 B/s');
    assert.strictEqual(formatSpeed(1048576), '1.00 MB/s');
    assert.strictEqual(formatSpeed(5242880), '5.00 MB/s');
  });
});

describe('formatEta', () => {
  test('formats seconds to MM:SS and HH:MM:SS', () => {
    assert.strictEqual(formatEta(null), '--:--');
    assert.strictEqual(formatEta(undefined), '--:--');
    assert.strictEqual(formatEta(-5), '--:--');
    assert.strictEqual(formatEta(45), '00:45');
    assert.strictEqual(formatEta(125), '02:05');
    assert.strictEqual(formatEta(3665), '1h 1m 5s');
  });
});

describe('calculateProgress', () => {
  test('calculates correct percentages and clamps properly', () => {
    assert.strictEqual(calculateProgress(0, 100), 0);
    assert.strictEqual(calculateProgress(50, 100), 50);
    assert.strictEqual(calculateProgress(100, 100), 100);
    assert.strictEqual(calculateProgress(150, 100), 100);
    assert.strictEqual(calculateProgress(50, 0), 0);
    assert.strictEqual(calculateProgress(50, null), 0);
  });
});

describe('calculateSpeed and calculateEta', () => {
  test('calculateSpeed computes delta over time', () => {
    assert.strictEqual(calculateSpeed(2000, 2), 1000);
    assert.strictEqual(calculateSpeed(0, 5), 0);
    assert.strictEqual(calculateSpeed(1000, 0), 0);
  });

  test('calculateEta computes remaining seconds', () => {
    assert.strictEqual(calculateEta(10000, 2000), 5);
    assert.strictEqual(calculateEta(0, 2000), 0);
    assert.strictEqual(calculateEta(1000, 0), null);
  });
});

describe('getFileCategory', () => {
  test('detects correct categories based on extension', () => {
    assert.strictEqual(getFileCategory('archive.zip').category, 'archive');
    assert.strictEqual(getFileCategory('video.mkv').category, 'video');
    assert.strictEqual(getFileCategory('audio.mp3').category, 'audio');
    assert.strictEqual(getFileCategory('doc.pdf').category, 'document');
    assert.strictEqual(getFileCategory('installer.exe').category, 'program');
    assert.strictEqual(getFileCategory('photo.png').category, 'image');
    assert.strictEqual(getFileCategory('unknown.xyz').category, 'other');
  });
});
