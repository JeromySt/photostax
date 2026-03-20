//! FFI-to-Rust bridge for foreign repository providers.
//!
//! This module converts C function pointer callbacks into a Rust
//! [`RepositoryProvider`] implementation that the core library can use.

use std::ffi::{CStr, CString};
use std::io::{self, Read, Seek, SeekFrom, Write};

use photostax_core::backends::foreign::RepositoryProvider;
use photostax_core::file_access::ReadSeek;
use photostax_core::scanner::FileEntry;

use crate::types::FfiProviderCallbacks;

/// A `RepositoryProvider` implementation backed by C function pointers.
///
/// This struct takes ownership of the `FfiProviderCallbacks` and forwards
/// all I/O operations to the host language via the function pointers.
///
/// # Safety
///
/// The `ctx` pointer and all callback function pointers in `callbacks` must
/// remain valid for the lifetime of this struct.
pub(crate) struct FfiRepositoryProvider {
    callbacks: FfiProviderCallbacks,
    location: String,
}

// SAFETY: The callbacks struct contains raw pointers and function pointers.
// The host guarantees these remain valid and thread-safe for the lifetime
// of the provider (documented in FfiProviderCallbacks).
unsafe impl Send for FfiRepositoryProvider {}
unsafe impl Sync for FfiRepositoryProvider {}

impl FfiRepositoryProvider {
    /// Create from raw FFI callbacks.
    ///
    /// # Safety
    ///
    /// - `callbacks.location` must be a valid null-terminated UTF-8 string
    /// - All function pointers must be valid
    /// - `callbacks.ctx` must remain valid until this provider is dropped
    pub(crate) unsafe fn new(callbacks: FfiProviderCallbacks) -> io::Result<Self> {
        let location = unsafe { CStr::from_ptr(callbacks.location) }
            .to_str()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid UTF-8 location"))?
            .to_string();

        Ok(Self {
            callbacks,
            location,
        })
    }
}

impl RepositoryProvider for FfiRepositoryProvider {
    fn location(&self) -> &str {
        &self.location
    }

    fn list_entries(&self, prefix: &str, recursive: bool) -> io::Result<Vec<FileEntry>> {
        let c_prefix = CString::new(prefix)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let result = unsafe {
            (self.callbacks.list_entries)(self.callbacks.ctx, c_prefix.as_ptr(), recursive)
        };

        if result.error != 0 {
            return Err(io::Error::other(format!(
                "list_entries failed with error code {}",
                result.error
            )));
        }

        let mut entries = Vec::with_capacity(result.len);
        if !result.data.is_null() && result.len > 0 {
            for i in 0..result.len {
                let ffi_entry = unsafe { &*result.data.add(i) };

                let name = unsafe { CStr::from_ptr(ffi_entry.name) }
                    .to_str()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8 name"))?
                    .to_string();

                let folder = unsafe { CStr::from_ptr(ffi_entry.folder) }
                    .to_str()
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8 folder")
                    })?
                    .to_string();

                let path = unsafe { CStr::from_ptr(ffi_entry.path) }
                    .to_str()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8 path"))?
                    .to_string();

                entries.push(FileEntry {
                    name,
                    folder,
                    path,
                    size: ffi_entry.size,
                });
            }
        }

        // Let the host free the entry array
        unsafe { (self.callbacks.free_entries)(self.callbacks.ctx, result) };

        Ok(entries)
    }

    fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        let c_path =
            CString::new(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let result =
            unsafe { (self.callbacks.open_read)(self.callbacks.ctx, c_path.as_ptr()) };

        if result.error != 0 || result.handle == 0 {
            return Err(io::Error::other(format!("open_read failed for '{path}'")));
        }

        Ok(Box::new(FfiReader {
            ctx: self.callbacks.ctx,
            handle: result.handle,
            read_fn: self.callbacks.read,
            seek_fn: self.callbacks.seek,
            close_fn: self.callbacks.close_read,
            closed: false,
        }))
    }

    fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        let c_path =
            CString::new(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let result =
            unsafe { (self.callbacks.open_write)(self.callbacks.ctx, c_path.as_ptr()) };

        if result.error != 0 || result.handle == 0 {
            return Err(io::Error::other(format!("open_write failed for '{path}'")));
        }

        Ok(Box::new(FfiWriter {
            ctx: self.callbacks.ctx,
            handle: result.handle,
            write_fn: self.callbacks.write,
            close_fn: self.callbacks.close_write,
            closed: false,
        }))
    }
}

/// A reader that delegates to FFI callback function pointers.
struct FfiReader {
    ctx: *mut std::os::raw::c_void,
    handle: u64,
    read_fn: unsafe extern "C" fn(
        *mut std::os::raw::c_void,
        u64,
        *mut u8,
        usize,
    ) -> crate::types::FfiReadResult,
    seek_fn: unsafe extern "C" fn(
        *mut std::os::raw::c_void,
        u64,
        i64,
        i32,
    ) -> crate::types::FfiSeekResult,
    close_fn: unsafe extern "C" fn(*mut std::os::raw::c_void, u64),
    closed: bool,
}

// SAFETY: The host guarantees thread-safety of callbacks.
unsafe impl Send for FfiReader {}

impl Read for FfiReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let result =
            unsafe { (self.read_fn)(self.ctx, self.handle, buf.as_mut_ptr(), buf.len()) };
        if result.error != 0 {
            return Err(io::Error::other("read callback failed"));
        }
        Ok(result.bytes_read)
    }
}

impl Seek for FfiReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, 0),
            SeekFrom::Current(n) => (n, 1),
            SeekFrom::End(n) => (n, 2),
        };

        let result = unsafe { (self.seek_fn)(self.ctx, self.handle, offset, whence) };
        if result.error != 0 {
            return Err(io::Error::other("seek callback failed"));
        }
        Ok(result.position)
    }
}

impl Drop for FfiReader {
    fn drop(&mut self) {
        if !self.closed {
            unsafe { (self.close_fn)(self.ctx, self.handle) };
            self.closed = true;
        }
    }
}

/// A writer that delegates to FFI callback function pointers.
struct FfiWriter {
    ctx: *mut std::os::raw::c_void,
    handle: u64,
    write_fn: unsafe extern "C" fn(
        *mut std::os::raw::c_void,
        u64,
        *const u8,
        usize,
    ) -> crate::types::FfiWriteResult,
    close_fn: unsafe extern "C" fn(*mut std::os::raw::c_void, u64),
    closed: bool,
}

// SAFETY: The host guarantees thread-safety of callbacks.
unsafe impl Send for FfiWriter {}

impl Write for FfiWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result =
            unsafe { (self.write_fn)(self.ctx, self.handle, buf.as_ptr(), buf.len()) };
        if result.error != 0 {
            return Err(io::Error::other("write callback failed"));
        }
        Ok(result.bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for FfiWriter {
    fn drop(&mut self) {
        if !self.closed {
            unsafe { (self.close_fn)(self.ctx, self.handle) };
            self.closed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::io::Cursor;
    use std::os::raw::c_char;
    use std::sync::Mutex;

    // ── Mock provider infrastructure ──────────────────────────────────────

    struct MockState {
        files: HashMap<String, Vec<u8>>,
        streams: HashMap<u64, Cursor<Vec<u8>>>,
        write_streams: HashMap<u64, (String, Vec<u8>)>,
        next_handle: u64,
    }

    fn setup_mock() -> *mut std::os::raw::c_void {
        let state = Box::new(Mutex::new(MockState {
            files: HashMap::new(),
            streams: HashMap::new(),
            write_streams: HashMap::new(),
            next_handle: 1,
        }));
        Box::into_raw(state) as *mut std::os::raw::c_void
    }

    fn add_mock_file(ctx: *mut std::os::raw::c_void, path: &str, content: &[u8]) {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();
        s.files.insert(path.to_string(), content.to_vec());
    }

    fn cleanup_mock(ctx: *mut std::os::raw::c_void) {
        unsafe {
            drop(Box::from_raw(ctx as *mut Mutex<MockState>));
        }
    }

    unsafe extern "C" fn mock_list_entries(
        ctx: *mut std::os::raw::c_void,
        _prefix: *const c_char,
        _recursive: bool,
    ) -> FfiFileEntryArray {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let s = state.lock().unwrap();

        if s.files.is_empty() {
            return FfiFileEntryArray {
                data: std::ptr::null(),
                len: 0,
                error: 0,
            };
        }

        // Allocate entries — leaked intentionally, freed by mock_free_entries
        let entries: Vec<FfiFileEntry> = s
            .files
            .iter()
            .map(|(path, content)| {
                let name = std::path::Path::new(path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let folder = std::path::Path::new(path)
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();

                FfiFileEntry {
                    name: CString::new(name).unwrap().into_raw() as *const c_char,
                    folder: CString::new(folder).unwrap().into_raw() as *const c_char,
                    path: CString::new(path.as_str()).unwrap().into_raw() as *const c_char,
                    size: content.len() as u64,
                }
            })
            .collect();

        let len = entries.len();
        let data = Box::into_raw(entries.into_boxed_slice()) as *const FfiFileEntry;

        FfiFileEntryArray {
            data,
            len,
            error: 0,
        }
    }

    unsafe extern "C" fn mock_free_entries(
        _ctx: *mut std::os::raw::c_void,
        entries: FfiFileEntryArray,
    ) {
        if entries.data.is_null() || entries.len == 0 {
            return;
        }
        // Reconstruct the boxed slice
        let boxed_slice = unsafe {
            Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                entries.data as *mut FfiFileEntry,
                entries.len,
            ))
        };
        // Free the CStrings in each entry
        for entry in boxed_slice.iter() {
            unsafe {
                drop(CString::from_raw(entry.name as *mut c_char));
                drop(CString::from_raw(entry.folder as *mut c_char));
                drop(CString::from_raw(entry.path as *mut c_char));
            }
        }
        // boxed_slice drops here, freeing the array memory
    }

    unsafe extern "C" fn mock_open_read(
        ctx: *mut std::os::raw::c_void,
        path: *const c_char,
    ) -> FfiStreamHandle {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let path_str = unsafe { CStr::from_ptr(path) }.to_str().unwrap();
        let mut s = state.lock().unwrap();

        let content = s.files.get(path_str).cloned();
        match content {
            Some(data) => {
                let handle = s.next_handle;
                s.next_handle += 1;
                s.streams.insert(handle, Cursor::new(data));
                FfiStreamHandle { handle, error: 0 }
            }
            None => FfiStreamHandle {
                handle: 0,
                error: 1,
            },
        }
    }

    unsafe extern "C" fn mock_read(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        buf: *mut u8,
        len: usize,
    ) -> FfiReadResult {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();

        match s.streams.get_mut(&handle) {
            Some(cursor) => {
                let dest = unsafe { std::slice::from_raw_parts_mut(buf, len) };
                match cursor.read(dest) {
                    Ok(n) => FfiReadResult {
                        bytes_read: n,
                        error: 0,
                    },
                    Err(_) => FfiReadResult {
                        bytes_read: 0,
                        error: 1,
                    },
                }
            }
            None => FfiReadResult {
                bytes_read: 0,
                error: 1,
            },
        }
    }

    unsafe extern "C" fn mock_seek(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        offset: i64,
        whence: i32,
    ) -> FfiSeekResult {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();

        match s.streams.get_mut(&handle) {
            Some(cursor) => {
                let pos = match whence {
                    0 => SeekFrom::Start(offset as u64),
                    1 => SeekFrom::Current(offset),
                    2 => SeekFrom::End(offset),
                    _ => {
                        return FfiSeekResult {
                            position: 0,
                            error: 1,
                        }
                    }
                };
                match cursor.seek(pos) {
                    Ok(p) => FfiSeekResult {
                        position: p,
                        error: 0,
                    },
                    Err(_) => FfiSeekResult {
                        position: 0,
                        error: 1,
                    },
                }
            }
            None => FfiSeekResult {
                position: 0,
                error: 1,
            },
        }
    }

    unsafe extern "C" fn mock_close_read(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
    ) {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();
        s.streams.remove(&handle);
    }

    unsafe extern "C" fn mock_open_write(
        ctx: *mut std::os::raw::c_void,
        path: *const c_char,
    ) -> FfiStreamHandle {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let path_str = unsafe { CStr::from_ptr(path) }.to_str().unwrap();
        let mut s = state.lock().unwrap();

        let handle = s.next_handle;
        s.next_handle += 1;
        s.write_streams
            .insert(handle, (path_str.to_string(), Vec::new()));
        FfiStreamHandle { handle, error: 0 }
    }

    unsafe extern "C" fn mock_write(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
        buf: *const u8,
        len: usize,
    ) -> FfiWriteResult {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();

        match s.write_streams.get_mut(&handle) {
            Some((_, ref mut buffer)) => {
                let data = unsafe { std::slice::from_raw_parts(buf, len) };
                buffer.extend_from_slice(data);
                FfiWriteResult {
                    bytes_written: len,
                    error: 0,
                }
            }
            None => FfiWriteResult {
                bytes_written: 0,
                error: 1,
            },
        }
    }

    unsafe extern "C" fn mock_close_write(
        ctx: *mut std::os::raw::c_void,
        handle: u64,
    ) {
        let state = unsafe { &*(ctx as *const Mutex<MockState>) };
        let mut s = state.lock().unwrap();
        if let Some((path, data)) = s.write_streams.remove(&handle) {
            s.files.insert(path, data);
        }
    }

    fn make_mock_callbacks(ctx: *mut std::os::raw::c_void) -> (FfiProviderCallbacks, *mut c_char) {
        let location = CString::new("mock://test").unwrap();
        let location_ptr = location.into_raw();
        let callbacks = FfiProviderCallbacks {
            ctx,
            location: location_ptr as *const c_char,
            list_entries: mock_list_entries,
            free_entries: mock_free_entries,
            open_read: mock_open_read,
            read: mock_read,
            seek: mock_seek,
            close_read: mock_close_read,
            open_write: mock_open_write,
            write: mock_write,
            close_write: mock_close_write,
        };
        (callbacks, location_ptr)
    }

    /// Drop provider first (closes any open streams), then free mock state and location.
    fn teardown(provider: FfiRepositoryProvider, ctx: *mut std::os::raw::c_void, loc: *mut c_char) {
        drop(provider);
        cleanup_mock(ctx);
        unsafe { drop(CString::from_raw(loc)); }
    }

    #[test]
    fn test_ffi_provider_list_entries_empty() {
        let ctx = setup_mock();
        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        let entries = provider.list_entries("", false).unwrap();
        assert!(entries.is_empty());

        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_list_entries_with_files() {
        let ctx = setup_mock();
        add_mock_file(ctx, "IMG_001.jpg", b"original");
        add_mock_file(ctx, "IMG_001_a.jpg", b"enhanced");

        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        let entries = provider.list_entries("", false).unwrap();
        assert_eq!(entries.len(), 2);

        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_open_read() {
        let ctx = setup_mock();
        add_mock_file(ctx, "test.txt", b"hello world");

        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        let mut reader = provider.open_read("test.txt").unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "hello world");

        drop(reader);
        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_open_read_not_found() {
        let ctx = setup_mock();
        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        let result = provider.open_read("nonexistent.txt");
        assert!(result.is_err());

        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_seek() {
        let ctx = setup_mock();
        add_mock_file(ctx, "data.bin", b"0123456789");

        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        let mut reader = provider.open_read("data.bin").unwrap();

        // Seek to position 5
        let pos = reader.seek(SeekFrom::Start(5)).unwrap();
        assert_eq!(pos, 5);

        // Read remaining
        let mut buf = [0u8; 5];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"56789");

        drop(reader);
        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_open_write() {
        let ctx = setup_mock();
        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        // Write a file
        let mut writer = provider.open_write("output.txt").unwrap();
        writer.write_all(b"written content").unwrap();
        drop(writer);

        // Read it back
        let mut reader = provider.open_read("output.txt").unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "written content");

        drop(reader);
        teardown(provider, ctx, loc);
    }

    #[test]
    fn test_ffi_provider_location() {
        let ctx = setup_mock();
        let (callbacks, loc) = make_mock_callbacks(ctx);
        let provider = unsafe { FfiRepositoryProvider::new(callbacks).unwrap() };

        assert_eq!(provider.location(), "mock://test");

        teardown(provider, ctx, loc);
    }
}
