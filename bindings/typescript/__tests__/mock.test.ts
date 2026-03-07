/**
 * Mock-based tests for index.ts
 *
 * These tests mock the native addon so every TypeScript code-path is exercised
 * regardless of whether the .node binary exists.
 */

// ──────────────────────────────────────────────────────────────────────
// We test the *module-level* code paths by controlling what `require` returns.
// jest.mock must be called before the import.
// ──────────────────────────────────────────────────────────────────────

// --- Scenario helpers ---------------------------------------------------

function makeFakeNativeStack(overrides: Record<string, unknown> = {}) {
  return {
    id: 'MOCK_001',
    original: '/photos/MOCK_001.jpg',
    enhanced: '/photos/MOCK_001_a.jpg',
    back: '/photos/MOCK_001_b.jpg',
    metadata: {
      exif_tags: { Make: 'EPSON' },
      xmp_tags: { Creator: 'Test' },
      custom_tags: { note: 'hello' },
    },
    ...overrides,
  };
}

// ────────────────────────────────────────────────────────────────────
// Group 1: Native module successfully loaded
// ────────────────────────────────────────────────────────────────────
describe('With mocked native addon (success path)', () => {
  let PhotostaxRepository: typeof import('../index').PhotostaxRepository;
  let isNativeAvailable: typeof import('../index').isNativeAvailable;
  let getNativeLoadError: typeof import('../index').getNativeLoadError;

  const mockScan = jest.fn();
  const mockGetStack = jest.fn();
  const mockReadImage = jest.fn();
  const mockWriteMetadata = jest.fn();
  const mockSearch = jest.fn();

  beforeAll(() => {
    // Provide a fake native binding
    jest.resetModules();
    jest.doMock('../photostax.node', () => ({
      PhotostaxRepository: class {
        constructor(_path: string) {}
        scan = mockScan;
        get_stack = mockGetStack;
        read_image = mockReadImage;
        write_metadata = mockWriteMetadata;
        search = mockSearch;
      },
    }), { virtual: true });
    // Re-import so module-level code picks up mock
    const mod = require('../index');
    PhotostaxRepository = mod.PhotostaxRepository;
    isNativeAvailable = mod.isNativeAvailable;
    getNativeLoadError = mod.getNativeLoadError;
  });

  afterAll(() => {
    jest.restoreAllMocks();
    jest.resetModules();
  });

  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('isNativeAvailable returns true', () => {
    expect(isNativeAvailable()).toBe(true);
  });

  it('getNativeLoadError returns null', () => {
    expect(getNativeLoadError()).toBeNull();
  });

  // ── constructor ──────────────────────────────────────────────────
  it('constructor succeeds with valid path', () => {
    expect(() => new PhotostaxRepository('/tmp')).not.toThrow();
  });

  // ── scan() ───────────────────────────────────────────────────────
  it('scan returns empty array', () => {
    mockScan.mockReturnValue([]);
    const repo = new PhotostaxRepository('/tmp');
    expect(repo.scan()).toEqual([]);
  });

  it('scan maps native stacks to TS PhotoStack', () => {
    mockScan.mockReturnValue([makeFakeNativeStack()]);
    const repo = new PhotostaxRepository('/tmp');
    const stacks = repo.scan();
    expect(stacks).toHaveLength(1);
    expect(stacks[0].id).toBe('MOCK_001');
    expect(stacks[0].original).toBe('/photos/MOCK_001.jpg');
    expect(stacks[0].enhanced).toBe('/photos/MOCK_001_a.jpg');
    expect(stacks[0].back).toBe('/photos/MOCK_001_b.jpg');
    expect(stacks[0].metadata.exifTags).toEqual({ Make: 'EPSON' });
    expect(stacks[0].metadata.xmpTags).toEqual({ Creator: 'Test' });
    expect(stacks[0].metadata.customTags).toEqual({ note: 'hello' });
  });

  it('scan handles null optional paths', () => {
    mockScan.mockReturnValue([
      makeFakeNativeStack({ original: null, enhanced: null, back: null }),
    ]);
    const repo = new PhotostaxRepository('/tmp');
    const stacks = repo.scan();
    expect(stacks[0].original).toBeNull();
    expect(stacks[0].enhanced).toBeNull();
    expect(stacks[0].back).toBeNull();
  });

  it('scan handles missing metadata sub-fields', () => {
    mockScan.mockReturnValue([
      makeFakeNativeStack({
        metadata: { exif_tags: undefined, xmp_tags: undefined, custom_tags: undefined },
      }),
    ]);
    const repo = new PhotostaxRepository('/tmp');
    const stacks = repo.scan();
    // fromNativeMetadata should default to {}
    expect(stacks[0].metadata.exifTags).toEqual({});
    expect(stacks[0].metadata.xmpTags).toEqual({});
    expect(stacks[0].metadata.customTags).toEqual({});
  });

  // ── getStack() ───────────────────────────────────────────────────
  it('getStack maps native to TS PhotoStack', () => {
    mockGetStack.mockReturnValue(makeFakeNativeStack({ id: 'MY_STACK' }));
    const repo = new PhotostaxRepository('/tmp');
    const stack = repo.getStack('MY_STACK');
    expect(stack.id).toBe('MY_STACK');
    expect(stack.metadata.exifTags.Make).toBe('EPSON');
  });

  it('getStack propagates native error', () => {
    mockGetStack.mockImplementation(() => {
      throw new Error('NotFound');
    });
    const repo = new PhotostaxRepository('/tmp');
    expect(() => repo.getStack('NOPE')).toThrow('NotFound');
  });

  // ── readImage() ──────────────────────────────────────────────────
  it('readImage returns buffer', () => {
    const buf = Buffer.from([0xff, 0xd8, 0xff]);
    mockReadImage.mockReturnValue(buf);
    const repo = new PhotostaxRepository('/tmp');
    const result = repo.readImage('/photos/MOCK_001.jpg');
    expect(result).toEqual(buf);
    expect(mockReadImage).toHaveBeenCalledWith('/photos/MOCK_001.jpg');
  });

  // ── writeMetadata() ──────────────────────────────────────────────
  it('writeMetadata converts camelCase to snake_case', () => {
    const repo = new PhotostaxRepository('/tmp');
    repo.writeMetadata('S1', {
      exifTags: { Make: 'Canon' },
      xmpTags: { Creator: 'Me' },
      customTags: { rating: 5 },
    });
    expect(mockWriteMetadata).toHaveBeenCalledWith('S1', {
      exif_tags: { Make: 'Canon' },
      xmp_tags: { Creator: 'Me' },
      custom_tags: { rating: 5 },
    });
  });

  it('writeMetadata defaults empty fields', () => {
    const repo = new PhotostaxRepository('/tmp');
    repo.writeMetadata('S1', {});
    expect(mockWriteMetadata).toHaveBeenCalledWith('S1', {
      exif_tags: {},
      xmp_tags: {},
      custom_tags: {},
    });
  });

  it('writeMetadata with partial metadata', () => {
    const repo = new PhotostaxRepository('/tmp');
    repo.writeMetadata('S1', { customTags: { foo: 'bar' } });
    expect(mockWriteMetadata).toHaveBeenCalledWith('S1', {
      exif_tags: {},
      xmp_tags: {},
      custom_tags: { foo: 'bar' },
    });
  });

  // ── search() ─────────────────────────────────────────────────────
  it('search with all query fields', () => {
    mockSearch.mockReturnValue([makeFakeNativeStack()]);
    const repo = new PhotostaxRepository('/tmp');
    const results = repo.search({
      text: 'birthday',
      exifFilters: [{ key: 'Make', value: 'EPSON' }],
      customFilters: [{ key: 'album', value: 'vacation' }],
      hasBack: true,
      hasEnhanced: false,
    });
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe('MOCK_001');
    expect(mockSearch).toHaveBeenCalledWith({
      text: 'birthday',
      exif_filters: [{ key: 'Make', value: 'EPSON' }],
      custom_filters: [{ key: 'album', value: 'vacation' }],
      has_back: true,
      has_enhanced: false,
    });
  });

  it('search with empty query defaults nulls', () => {
    mockSearch.mockReturnValue([]);
    const repo = new PhotostaxRepository('/tmp');
    repo.search({});
    expect(mockSearch).toHaveBeenCalledWith({
      text: null,
      exif_filters: null,
      custom_filters: null,
      has_back: null,
      has_enhanced: null,
    });
  });

  it('search with text only', () => {
    mockSearch.mockReturnValue([]);
    const repo = new PhotostaxRepository('/tmp');
    repo.search({ text: 'hello' });
    expect(mockSearch).toHaveBeenCalledWith({
      text: 'hello',
      exif_filters: null,
      custom_filters: null,
      has_back: null,
      has_enhanced: null,
    });
  });

  it('search maps multiple results', () => {
    mockSearch.mockReturnValue([
      makeFakeNativeStack({ id: 'A' }),
      makeFakeNativeStack({ id: 'B' }),
    ]);
    const repo = new PhotostaxRepository('/tmp');
    const results = repo.search({ text: '' });
    expect(results).toHaveLength(2);
    expect(results[0].id).toBe('A');
    expect(results[1].id).toBe('B');
  });
});

// ────────────────────────────────────────────────────────────────────
// Group 2: Native module fails to load (Error path)
// ────────────────────────────────────────────────────────────────────
describe('With native addon load failure (Error)', () => {
  let PhotostaxRepository: typeof import('../index').PhotostaxRepository;
  let isNativeAvailable: typeof import('../index').isNativeAvailable;
  let getNativeLoadError: typeof import('../index').getNativeLoadError;

  beforeAll(() => {
    jest.resetModules();
    jest.doMock('../photostax.node', () => {
      throw new Error('Cannot find module');
    }, { virtual: true });
    const mod = require('../index');
    PhotostaxRepository = mod.PhotostaxRepository;
    isNativeAvailable = mod.isNativeAvailable;
    getNativeLoadError = mod.getNativeLoadError;
  });

  afterAll(() => {
    jest.restoreAllMocks();
    jest.resetModules();
  });

  it('isNativeAvailable returns false', () => {
    expect(isNativeAvailable()).toBe(false);
  });

  it('getNativeLoadError returns an Error', () => {
    const err = getNativeLoadError();
    expect(err).toBeInstanceOf(Error);
    expect(err!.message).toContain('Cannot find module');
  });

  it('constructor throws with meaningful message', () => {
    expect(() => new PhotostaxRepository('/tmp')).toThrow(/Native addon not loaded/);
    expect(() => new PhotostaxRepository('/tmp')).toThrow(/Cannot find module/);
  });
});

// ────────────────────────────────────────────────────────────────────
// Group 3: Native module throws non-Error (string)
// ────────────────────────────────────────────────────────────────────
describe('With native addon load failure (non-Error throw)', () => {
  let isNativeAvailable: typeof import('../index').isNativeAvailable;
  let getNativeLoadError: typeof import('../index').getNativeLoadError;
  let PhotostaxRepository: typeof import('../index').PhotostaxRepository;

  beforeAll(() => {
    jest.resetModules();
    jest.doMock('../photostax.node', () => {
      throw 'string error thrown'; // eslint-disable-line no-throw-literal
    }, { virtual: true });
    const mod = require('../index');
    isNativeAvailable = mod.isNativeAvailable;
    getNativeLoadError = mod.getNativeLoadError;
    PhotostaxRepository = mod.PhotostaxRepository;
  });

  afterAll(() => {
    jest.restoreAllMocks();
    jest.resetModules();
  });

  it('wraps non-Error in new Error()', () => {
    expect(isNativeAvailable()).toBe(false);
    const err = getNativeLoadError();
    expect(err).toBeInstanceOf(Error);
    expect(err!.message).toContain('string error thrown');
  });

  it('constructor error includes "unknown" when loadError message is empty-ish', () => {
    expect(() => new PhotostaxRepository('/tmp')).toThrow(/Native addon not loaded/);
  });
});

// ────────────────────────────────────────────────────────────────────
// Group 4: loadError is null but nativeBinding is also null
// (defensive edge: covers ?? 'unknown' branch)
// ────────────────────────────────────────────────────────────────────
describe('With null loadError and null nativeBinding', () => {
  let PhotostaxRepository: typeof import('../index').PhotostaxRepository;

  beforeAll(() => {
    jest.resetModules();
    // Mock require to NOT throw but return something falsy
    jest.doMock('../photostax.node', () => null, { virtual: true });
    const mod = require('../index');
    PhotostaxRepository = mod.PhotostaxRepository;
  });

  afterAll(() => {
    jest.restoreAllMocks();
    jest.resetModules();
  });

  it('constructor throws with "unknown" when loadError is null', () => {
    expect(() => new PhotostaxRepository('/tmp')).toThrow(/unknown/);
  });
});
