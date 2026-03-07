/**
 * Tests for metadata operations
 *
 * Tests require the native addon to be built. Skipped if not available.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { isNativeAvailable, PhotostaxRepository, Metadata } from '../index';

const describeWithNative = isNativeAvailable() ? describe : describe.skip;

describeWithNative('PhotostaxRepository.writeMetadata()', () => {
  let tempDir: string;

  beforeAll(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'photostax-metadata-test-'));

    // Create minimal JPEG files
    const minimalJpeg = Buffer.from([
      0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
      0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xd9,
    ]);

    fs.writeFileSync(path.join(tempDir, 'Meta_001.jpg'), minimalJpeg);
    fs.writeFileSync(path.join(tempDir, 'Meta_001_a.jpg'), minimalJpeg);
  });

  afterAll(() => {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  it('should write custom tags to sidecar database', () => {
    const repo = new PhotostaxRepository(tempDir);

    // Write metadata
    repo.writeMetadata('Meta_001', {
      customTags: {
        album: 'Test Album',
        rating: 5,
      },
    });

    // Verify by scanning and checking the stack
    const stack = repo.getStack('Meta_001');
    expect(stack.metadata.customTags['album']).toBe('Test Album');
    expect(stack.metadata.customTags['rating']).toBe(5);
  });

  it('should allow partial metadata updates', () => {
    const repo = new PhotostaxRepository(tempDir);

    // First update
    repo.writeMetadata('Meta_001', {
      customTags: { tag1: 'value1' },
    });

    // Second update (should not overwrite first)
    repo.writeMetadata('Meta_001', {
      customTags: { tag2: 'value2' },
    });

    const stack = repo.getStack('Meta_001');
    expect(stack.metadata.customTags['tag1']).toBe('value1');
    expect(stack.metadata.customTags['tag2']).toBe('value2');
  });

  it('should throw for non-existent stack', () => {
    const repo = new PhotostaxRepository(tempDir);
    expect(() =>
      repo.writeMetadata('NONEXISTENT_999', {
        customTags: { test: 'value' },
      })
    ).toThrow();
  });
});

describe('Metadata type checks', () => {
  it('should have correct Metadata interface shape', () => {
    const metadata: Metadata = {
      exifTags: { Make: 'EPSON' },
      xmpTags: { description: 'Test' },
      customTags: { album: 'Family', count: 42, nested: { key: 'value' } },
    };

    expect(metadata.exifTags['Make']).toBe('EPSON');
    expect(metadata.xmpTags['description']).toBe('Test');
    expect(metadata.customTags['album']).toBe('Family');
    expect(metadata.customTags['count']).toBe(42);
  });
});
