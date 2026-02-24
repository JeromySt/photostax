/**
 * Tests for PhotostaxRepository
 *
 * These tests verify the repository functionality. Tests requiring the native addon
 * will be skipped if it's not available.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { isNativeAvailable, getNativeLoadError, PhotostaxRepository } from '../index';

// Skip tests if native addon is not available
const describeWithNative = isNativeAvailable() ? describe : describe.skip;

describe('Native addon loading', () => {
  it('should report whether native addon is available', () => {
    const available = isNativeAvailable();
    expect(typeof available).toBe('boolean');
  });

  it('should provide error message if native addon failed to load', () => {
    const error = getNativeLoadError();
    if (!isNativeAvailable()) {
      expect(error).toBeInstanceOf(Error);
      expect(error?.message).toBeDefined();
    } else {
      expect(error).toBeNull();
    }
  });
});

describe('PhotostaxRepository constructor', () => {
  it('should throw if native addon is not available', () => {
    if (!isNativeAvailable()) {
      expect(() => new PhotostaxRepository('/tmp/test')).toThrow(/Native addon not loaded/);
    } else {
      // Native is available, constructor should work
      expect(() => new PhotostaxRepository(os.tmpdir())).not.toThrow();
    }
  });
});

describeWithNative('PhotostaxRepository with native addon', () => {
  let tempDir: string;

  beforeAll(() => {
    // Create a temporary directory for test fixtures
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'photostax-test-'));
  });

  afterAll(() => {
    // Clean up test directory
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('scan()', () => {
    it('should return empty array for empty directory', () => {
      const repo = new PhotostaxRepository(tempDir);
      const stacks = repo.scan();
      expect(stacks).toEqual([]);
    });

    it('should discover photo stacks from files', () => {
      // Create test JPEG files (minimal valid JPEG)
      const minimalJpeg = Buffer.from([
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xd9,
      ]);

      fs.writeFileSync(path.join(tempDir, 'IMG_001.jpg'), minimalJpeg);
      fs.writeFileSync(path.join(tempDir, 'IMG_001_a.jpg'), minimalJpeg);
      fs.writeFileSync(path.join(tempDir, 'IMG_001_b.jpg'), minimalJpeg);

      const repo = new PhotostaxRepository(tempDir);
      const stacks = repo.scan();

      expect(stacks.length).toBeGreaterThanOrEqual(1);

      const stack = stacks.find((s) => s.id === 'IMG_001');
      expect(stack).toBeDefined();
      expect(stack?.original).toContain('IMG_001.jpg');
      expect(stack?.enhanced).toContain('IMG_001_a.jpg');
      expect(stack?.back).toContain('IMG_001_b.jpg');
    });

    it('should include metadata object', () => {
      const repo = new PhotostaxRepository(tempDir);
      const stacks = repo.scan();

      for (const stack of stacks) {
        expect(stack.metadata).toBeDefined();
        expect(stack.metadata.exifTags).toBeDefined();
        expect(stack.metadata.xmpTags).toBeDefined();
        expect(stack.metadata.customTags).toBeDefined();
      }
    });
  });

  describe('getStack()', () => {
    beforeAll(() => {
      // Ensure test files exist
      const minimalJpeg = Buffer.from([
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xd9,
      ]);
      fs.writeFileSync(path.join(tempDir, 'TEST_002.jpg'), minimalJpeg);
    });

    it('should retrieve a specific stack by ID', () => {
      const repo = new PhotostaxRepository(tempDir);
      const stack = repo.getStack('TEST_002');

      expect(stack).toBeDefined();
      expect(stack.id).toBe('TEST_002');
    });

    it('should throw for non-existent stack', () => {
      const repo = new PhotostaxRepository(tempDir);
      expect(() => repo.getStack('NONEXISTENT_999')).toThrow();
    });
  });

  describe('readImage()', () => {
    it('should read image bytes', () => {
      const testPath = path.join(tempDir, 'IMG_001.jpg');
      const repo = new PhotostaxRepository(tempDir);
      const buffer = repo.readImage(testPath);

      expect(buffer).toBeInstanceOf(Buffer);
      expect(buffer.length).toBeGreaterThan(0);
      // Check JPEG magic bytes
      expect(buffer[0]).toBe(0xff);
      expect(buffer[1]).toBe(0xd8);
    });

    it('should throw for non-existent file', () => {
      const repo = new PhotostaxRepository(tempDir);
      expect(() => repo.readImage(path.join(tempDir, 'nonexistent.jpg'))).toThrow();
    });
  });
});
