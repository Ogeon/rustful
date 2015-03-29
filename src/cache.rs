//!Traits and implementations for cached resources.

#![stable]

use std::io::{self, Read};
use std::path::Path;
use std::fs::{File, PathExt};
use std::sync::{RwLock, RwLockReadGuard};

use time;
use time::Timespec;

use log::Log;

macro_rules! unwrap_mutex {
    ($log:ident, $mutex:expr) => (
        match $mutex {
            Ok(guard) => guard,
            Err(poisoned) => {
                $log.warning("poisoned mutex");
                poisoned.into_inner()
            }
        }
    )
}

///A trait for cache storage.
#[unstable]
pub trait Cache {
    ///Free all the unused cached resources.
    fn free_unused(&self, log: &Log);
}

impl Cache for () {
    fn free_unused(&self, _log: &Log) {}
}


///This trait provides functions for handling cached resources.
#[unstable]
pub trait CachedValue<'a, Value> {

    ///Borrow the cached value, without loading or reloading it.
    fn borrow_current(&'a self, log: &Log) -> Value;

    ///Load the cached value.
    fn load(&self, log: &Log);

    ///Free the cached value.
    fn free(&self, log: &Log);

    ///Check if the cached value has expired.
    fn expired(&self, log: &Log) -> bool;

    ///Check if the cached value is unused and should be removed.
    fn unused(&self, log: &Log) -> bool;

    ///Reload the cached value if it has expired and borrow it.
    fn borrow(&'a self, log: &Log) -> Value {
        if self.expired(log) {
            self.load(log);
        }

        self.borrow_current(log)
    }

    ///Free the cached value if it's unused.
    fn clean(&self, log: &Log) {
        if self.unused(log) {
            self.free(log);
        }
    }
}

///Cached raw file content.
///
///The whole file will be loaded when accessed.
///
///```rust
///use std::path::Path;
///use rustful::cache::{CachedValue, CachedFile};
///# use rustful::log::{Log, StdOut};
///# let log = &StdOut as &Log;
///
///let file = CachedFile::new(&Path::new("/some/file/path.txt"), None);
///let guard = file.borrow(log);
///match *guard {
///    Some(ref content) => println!("loaded file with {} bytes of data", content.len()),
///    None => println!("the file was not loaded")
///}
///```
#[unstable]
pub struct CachedFile<'p> {
    path: &'p Path,
    file: RwLock<Option<Vec<u8>>>,
    modified: RwLock<u64>,
    last_accessed: RwLock<Timespec>,
    unused_after: Option<i64>
}

#[unstable]
impl<'p> CachedFile<'p> {
    ///Creates a new `CachedFile` which will be freed `unused_after` seconds after the latest access.
    pub fn new(path: &'p Path, unused_after: Option<u32>) -> CachedFile<'p> {
        CachedFile {
            path: path,
            file: RwLock::new(None),
            modified: RwLock::new(0),
            last_accessed: RwLock::new(Timespec::new(0, 0)),
            unused_after: unused_after.map(|i| i as i64),
        }
    }
}

impl<'a, 'p> CachedValue<'a, RwLockReadGuard<'a, Option<Vec<u8>>>> for CachedFile<'p> {
    fn borrow_current(&'a self, log: &Log) -> RwLockReadGuard<'a, Option<Vec<u8>>> {
        if self.unused_after.is_some() {
            *unwrap_mutex!(log, self.last_accessed.write()) = time::get_time();
        }
        
        unwrap_mutex!(log, self.file.read())
    }

    fn load(&self, log: &Log) {
        *unwrap_mutex!(log, self.modified.write()) = self.path.metadata().map(|m| m.modified()).unwrap_or(0);
        let mut buf = Vec::new();
        let res = match File::open(self.path) {
            Ok(ref mut f) => f.read_to_end(&mut buf),
            Err(e) => {
                log.error(&format!("could not open file {}: {}", self.path.display(), e));
                Err(e)
            }
        };
        *unwrap_mutex!(log, self.file.write()) = res.ok().map(|_| buf);

        if self.unused_after.is_some() {
            *unwrap_mutex!(log, self.last_accessed.write()) = time::get_time();
        }
    }

    fn free(&self, log: &Log) {
        *unwrap_mutex!(log, self.file.write()) = None;
    }

    fn expired(&self, log: &Log) -> bool {
        if unwrap_mutex!(log, self.file.read()).is_some() {
            self.path.metadata().map(|m| m.modified() > *unwrap_mutex!(log, self.modified.read())).unwrap_or(false)
        } else {
            true
        }
    }

    fn unused(&self, log: &Log) -> bool {
        if unwrap_mutex!(log, self.file.read()).is_some() {
            self.unused_after.map(|t| {
                let last_accessed = unwrap_mutex!(log, self.last_accessed.read());
                let unused_time = Timespec::new(last_accessed.sec + t, last_accessed.nsec);
                time::get_time() > unused_time
            }).unwrap_or(false)
        } else {
            false
        }
    }
}


///A processed cached file.
///
///The file will be processed by a provided function
///each time it is loaded and the result will be stored.
///
///```rust
///use std::io;
///use std::path::Path;
///use std::fs::{File, PathExt};
///use rustful::cache::{CachedValue, CachedProcessedFile};
///# use rustful::log::{Log, StdOut};
///# let log = &StdOut as &Log;
///
///fn get_size(_log: &Log, file: io::Result<File>) -> io::Result<Option<u64>> {
///    file.and_then(|mut file| file.metadata()).map(|m| Some(m.len()))
///}
///
///let file = CachedProcessedFile::new(&Path::new("/some/file/path.txt"), None, get_size);
///let guard = file.borrow(log);
///match *guard {
///    Some(ref size) => println!("file contains {} bytes of data", size),
///    None => println!("the file was not loaded")
///}
///```
#[unstable]
pub struct CachedProcessedFile<'p, T> {
    path: &'p Path,
    file: RwLock<Option<T>>,
    modified: RwLock<u64>,
    last_accessed: RwLock<Timespec>,
    unused_after: Option<i64>,
    processor: fn(&Log, io::Result<File>) -> io::Result<Option<T>>
}

#[unstable]
impl<'p, T: Send+Sync> CachedProcessedFile<'p, T> {
    ///Creates a new `CachedProcessedFile` which will be freed `unused_after` seconds after the latest access.
    ///The file will be processed by the provided `processor` function each time it's loaded.
    pub fn new(path: &'p Path, unused_after: Option<u32>, processor: fn(&Log, io::Result<File>) -> io::Result<Option<T>>) -> CachedProcessedFile<'p, T> {
        CachedProcessedFile {
            path: path,
            file: RwLock::new(None),
            modified: RwLock::new(0),
            last_accessed: RwLock::new(Timespec::new(0, 0)),
            unused_after: unused_after.map(|i| i as i64),
            processor: processor
        }
    }
}

impl<'a, 'p, T: Send+Sync> CachedValue<'a, RwLockReadGuard<'a, Option<T>>> for CachedProcessedFile<'p, T> {
    fn borrow_current(&'a self, log: &Log) -> RwLockReadGuard<'a, Option<T>> {
        if self.unused_after.is_some() {
            *unwrap_mutex!(log, self.last_accessed.write()) = time::get_time();
        }

        unwrap_mutex!(log, self.file.read())
    }

    fn load(&self, log: &Log) {
        *unwrap_mutex!(log, self.modified.write()) = self.path.metadata().map(|m| m.modified()).unwrap_or(0);
        *unwrap_mutex!(log, self.file.write()) = (self.processor)(log, File::open(&self.path)).ok().and_then(|result| result);

        if self.unused_after.is_some() {
            *unwrap_mutex!(log, self.last_accessed.write()) = time::get_time();
        }
    }

    fn free(&self, log: &Log) {
        *unwrap_mutex!(log, self.file.write()) = None;
    }

    fn expired(&self, log: &Log) -> bool {
        if unwrap_mutex!(log, self.file.read()).is_some() {
            self.path.metadata().map(|m| m.modified() > *unwrap_mutex!(log, self.modified.read())).unwrap_or(true)
        } else {
            true
        }
    }

    fn unused(&self, log: &Log) -> bool {
        if unwrap_mutex!(log, self.file.read()).is_some() {
            self.unused_after.map(|t| {
                let last_accessed = unwrap_mutex!(log, self.last_accessed.read());
                let unused_time = Timespec::new(last_accessed.sec + t, last_accessed.nsec);
                time::get_time() > unused_time
            }).unwrap_or(false)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::{self, Read};
    use std::fs::File;
    use std::path::Path;
    use cache::{CachedValue, CachedFile, CachedProcessedFile};
    use log::{Log, Result};

    struct DummyLog;

    impl Log for DummyLog {
        fn try_note(&self, _message: &str) -> Result {
            Ok(())
        }
        fn try_warning(&self, _message: &str) -> Result {
            Ok(())
        }
        fn try_error(&self, _message: &str) -> Result {
            Ok(())
        }
    }

    #[test]
    fn file() {
        let log = &DummyLog;
        let file = CachedFile::new(Path::new("LICENSE"), None);
        assert_eq!(file.expired(log), true);
        assert!(file.borrow(log).as_ref().map(|v| v.len()).unwrap_or(0) > 0);
        assert_eq!(file.expired(log), false);
        file.free(log);
        assert_eq!(file.expired(log), true);
    }

    #[test]
    fn modified_file() {
        fn just_read(_log: &Log, file: io::Result<File>) -> io::Result<Option<Vec<u8>>> {
            let mut buf = Vec::new();
            try!(file).read_to_end(&mut buf).map(|_| Some(buf))
        }

        let log = &DummyLog;
        let file = CachedProcessedFile::new(Path::new("LICENSE"), None, just_read);
        assert_eq!(file.expired(log), true);
        assert!(file.borrow(log).as_ref().map(|v| v.len()).unwrap_or(0) > 0);
        assert_eq!(file.expired(log), false);
        file.free(log);
        assert_eq!(file.expired(log), true);
    }
}
