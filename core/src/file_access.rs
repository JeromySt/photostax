//! File access abstraction with locking semantics.
//!
//! The [`FileAccess`] trait provides polymorphic file I/O that each
//! repository backend implements with its own locking strategy:
//!
//! - Local filesystem: OS-level file locks
//! - Cloud storage: blob leases, ETags, or API-level locks

use std::io::{self, Read, Seek, Write};

use sha2::{Digest, Sha256};

/// A readable and seekable stream.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// File access with backend-specific locking semantics.
///
/// ## Locking contract
///
/// | Scenario | Behavior |
/// |----------|----------|
/// | Read while others reading | ✅ Shared — concurrent OK |
/// | Write while others reading | ⏳ Blocks until readers close |
/// | Read while writer holds lock | ⏳ Blocks until writer closes |
/// | Write while writer holds lock | ⏳ Blocks (single writer) |
pub trait FileAccess {
    /// Open a file for shared concurrent reading.
    /// Multiple readers can hold this simultaneously.
    fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>>;

    /// Open a file for exclusive writing.
    /// Blocks until all readers close, then acquires exclusive lock.
    fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>>;

    /// Compute content hash by streaming through SHA-256.
    /// Default implementation uses `open_read()` — works for any backend.
    fn hash_file(&self, path: &str) -> io::Result<String> {
        let mut reader = self.open_read(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let hash = hasher.finalize();
        Ok(format!("{:x}", hash)[..16].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    struct TestAccess;

    impl FileAccess for TestAccess {
        fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
            let file = std::fs::File::open(path)?;
            Ok(Box::new(std::io::BufReader::new(file)))
        }

        fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            Ok(Box::new(file))
        }
    }

    #[test]
    fn test_open_read_returns_file_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, b"hello world").unwrap();

        let access = TestAccess;
        let mut reader = access.open_read(path.to_str().unwrap()).unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_open_write_creates_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("out.txt");

        let access = TestAccess;
        let mut writer = access.open_write(path.to_str().unwrap()).unwrap();
        writer.write_all(b"written").unwrap();
        drop(writer);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "written");
    }

    #[test]
    fn test_hash_file_default_impl() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("hash_me.bin");
        std::fs::write(&path, b"deterministic content").unwrap();

        let access = TestAccess;
        let hash1 = access.hash_file(path.to_str().unwrap()).unwrap();
        let hash2 = access.hash_file(path.to_str().unwrap()).unwrap();

        assert_eq!(hash1.len(), 16);
        assert_eq!(hash1, hash2); // deterministic
    }

    #[test]
    fn test_concurrent_reads() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("shared.txt");
        std::fs::write(&path, b"shared content").unwrap();

        let access = TestAccess;
        let path_str = path.to_str().unwrap();

        // Multiple readers can be open simultaneously
        let mut r1 = access.open_read(path_str).unwrap();
        let mut r2 = access.open_read(path_str).unwrap();

        let mut c1 = String::new();
        let mut c2 = String::new();
        r1.read_to_string(&mut c1).unwrap();
        r2.read_to_string(&mut c2).unwrap();

        assert_eq!(c1, c2);
        assert_eq!(c1, "shared content");
    }
}
