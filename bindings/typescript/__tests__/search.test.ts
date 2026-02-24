/**
 * Tests for search functionality
 *
 * Tests require the native addon to be built. Skipped if not available.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { isNativeAvailable, PhotostaxRepository, SearchQuery } from '../index';

const describeWithNative = isNativeAvailable() ? describe : describe.skip;

describeWithNative('PhotostaxRepository.search()', () => {
  let tempDir: string;

  beforeAll(() => {
    // Create a temporary directory with test fixtures
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'photostax-search-test-'));

    // Create minimal JPEG files
    const minimalJpeg = Buffer.from([
      0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
      0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xd9,
    ]);

    // Create several test stacks
    fs.writeFileSync(path.join(tempDir, 'Family_001.jpg'), minimalJpeg);
    fs.writeFileSync(path.join(tempDir, 'Family_001_a.jpg'), minimalJpeg);
    fs.writeFileSync(path.join(tempDir, 'Family_001_b.jpg'), minimalJpeg);

    fs.writeFileSync(path.join(tempDir, 'Vacation_002.jpg'), minimalJpeg);
    fs.writeFileSync(path.join(tempDir, 'Vacation_002_a.jpg'), minimalJpeg);
    // No back for Vacation_002

    fs.writeFileSync(path.join(tempDir, 'Wedding_003.jpg'), minimalJpeg);
    // No enhanced or back for Wedding_003
  });

  afterAll(() => {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  it('should return all stacks with empty query', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({});
    expect(results.length).toBe(3);
  });

  it('should filter by text in stack ID', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ text: 'Family' });
    expect(results.length).toBe(1);
    expect(results[0].id).toBe('Family_001');
  });

  it('should filter by hasBack=true', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ hasBack: true });
    expect(results.length).toBe(1);
    expect(results[0].id).toBe('Family_001');
  });

  it('should filter by hasBack=false', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ hasBack: false });
    expect(results.length).toBe(2);
    const ids = results.map((s) => s.id).sort();
    expect(ids).toEqual(['Vacation_002', 'Wedding_003']);
  });

  it('should filter by hasEnhanced=true', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ hasEnhanced: true });
    expect(results.length).toBe(2);
    const ids = results.map((s) => s.id).sort();
    expect(ids).toEqual(['Family_001', 'Vacation_002']);
  });

  it('should filter by hasEnhanced=false', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ hasEnhanced: false });
    expect(results.length).toBe(1);
    expect(results[0].id).toBe('Wedding_003');
  });

  it('should combine multiple filters with AND logic', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({
      hasEnhanced: true,
      hasBack: true,
    });
    expect(results.length).toBe(1);
    expect(results[0].id).toBe('Family_001');
  });

  it('should return empty array when no stacks match', () => {
    const repo = new PhotostaxRepository(tempDir);
    const results = repo.search({ text: 'Nonexistent' });
    expect(results).toEqual([]);
  });
});
