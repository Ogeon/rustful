//!Utility traits and implementations for cached resources.

use std::io::{File, IoResult};
use std::io::fs::PathExtensions;
use std::sync::{RwLock, RwLockReadGuard};

use time;
use time::Timespec;


///This trait provides functions for handling cached resources.
pub trait CachedValue<'a, Value> {

	///Borrow the cached value, without loading or reloading it.
	fn borrow_current(&'a self) -> Value;

	///Load the cached value.
	fn load(&self);

	///Free the cached value.
	fn free(&self);

	///Check if the cached value has expired.
	fn expired(&self) -> bool;

	///Check if the cached value is unused and should be removed.
	fn unused(&self) -> bool;

	///Reload the cached value if it has expired and borrow it.
	fn borrow(&'a self) -> Value {
		if self.expired() {
			self.load();
		}

		self.borrow_current()
	}

	///Free the cached value if it's unused.
	fn clean(&self) {
		if self.unused() {
			self.free();
		}
	}
}

///Cached raw file content.
///
///The whole file will be loaded when accessed.
///
///```rust
///# #![allow(unstable)]
///use rustful::cache::{CachedValue, CachedFile};
///
///let file = CachedFile::new(Path::new("/some/file/path.txt"), None);
///
///match *file.borrow() {
///    Some(ref content) => println!("loaded file with {} bytes of data", content.len()),
///    None => println!("the file was not loaded")
///}
///```
pub struct CachedFile {
	path: Path,
	file: RwLock<Option<Vec<u8>>>,
	modified: RwLock<u64>,
	last_accessed: RwLock<Timespec>,
	unused_after: Option<i64>
}

impl CachedFile {
	///Creates a new `CachedFile` which will be freed `unused_after` seconds after the latest access.
	pub fn new(path: Path, unused_after: Option<u32>) -> CachedFile {
		CachedFile {
			path: path,
			file: RwLock::new(None),
			modified: RwLock::new(0),
			last_accessed: RwLock::new(Timespec::new(0, 0)),
			unused_after: unused_after.map(|i| i as i64),
		}
	}
}

impl<'a> CachedValue<'a, RwLockReadGuard<'a, Option<Vec<u8>>>> for CachedFile {
	fn borrow_current(&'a self) -> RwLockReadGuard<'a, Option<Vec<u8>>> {
		if self.unused_after.is_some() {
			*self.last_accessed.write().unwrap() = time::get_time();
		}
		
		self.file.read().unwrap()
	}

	fn load(&self) {
		*self.modified.write().unwrap() = self.path.stat().map(|s| s.modified).unwrap_or(0);
		*self.file.write().unwrap() = File::open(&self.path).read_to_end().ok();

		if self.unused_after.is_some() {
			*self.last_accessed.write().unwrap() = time::get_time();
		}
	}

	fn free(&self) {
		*self.file.write().unwrap() = None;
	}

	fn expired(&self) -> bool {
		if self.file.read().unwrap().is_some() {
			self.path.stat().map(|s| s.modified > *self.modified.read().unwrap()).unwrap_or(false)
		} else {
			true
		}
	}

	fn unused(&self) -> bool {
		if self.file.read().unwrap().is_some() {
			self.unused_after.map(|t| {
				let last_accessed = self.last_accessed.read().unwrap();
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
///# #![allow(unstable)]
///use std::io::{File, IoResult};
///use rustful::cache::{CachedValue, CachedProcessedFile};
///
///fn get_size(file: IoResult<File>) -> IoResult<Option<u64>> {
///    file.and_then(|mut file| file.stat()).map(|stat| Some(stat.size))
///}
///
///let file = CachedProcessedFile::new(Path::new("/some/file/path.txt"), None, get_size);
///
///match *file.borrow() {
///    Some(ref size) => println!("file contains {} bytes of data", size),
///    None => println!("the file was not loaded")
///}
///```
pub struct CachedProcessedFile<T> {
	path: Path,
	file: RwLock<Option<T>>,
	modified: RwLock<u64>,
	last_accessed: RwLock<Timespec>,
	unused_after: Option<i64>,
	processor: fn(IoResult<File>) -> IoResult<Option<T>>
}

impl<T: Send+Sync> CachedProcessedFile<T> {
	///Creates a new `CachedProcessedFile` which will be freed `unused_after` seconds after the latest access.
	///The file will be processed by the provided `processor` function each time it's loaded.
	pub fn new(path: Path, unused_after: Option<u32>, processor: fn(IoResult<File>) -> IoResult<Option<T>>) -> CachedProcessedFile<T> {
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

impl<'a, T: Send+Sync> CachedValue<'a, RwLockReadGuard<'a, Option<T>>> for CachedProcessedFile<T> {
	fn borrow_current(&'a self) -> RwLockReadGuard<'a, Option<T>> {
		if self.unused_after.is_some() {
			*self.last_accessed.write().unwrap() = time::get_time();
		}

		self.file.read().unwrap()
	}

	fn load(&self) {
		*self.modified.write().unwrap() = self.path.stat().map(|s| s.modified).unwrap_or(0);
		*self.file.write().unwrap() = (self.processor)(File::open(&self.path)).ok().and_then(|result| result);

		if self.unused_after.is_some() {
			*self.last_accessed.write().unwrap() = time::get_time();
		}
	}

	fn free(&self) {
		*self.file.write().unwrap() = None;
	}

	fn expired(&self) -> bool {
		if self.file.read().unwrap().is_some() {
			self.path.stat().map(|s| s.modified > *self.modified.read().unwrap()).unwrap_or(true)
		} else {
			true
		}
	}

	fn unused(&self) -> bool {
		if self.file.read().unwrap().is_some() {
			self.unused_after.map(|t| {
				let last_accessed = self.last_accessed.read().unwrap();
				let unused_time = Timespec::new(last_accessed.sec + t, last_accessed.nsec);
				time::get_time() > unused_time
			}).unwrap_or(false)
		} else {
			false
		}
	}
}



#[test]
fn file() {
	let file = CachedFile::new(Path::new("LICENSE"), None);
	assert_eq!(file.expired(), true);
	assert!(file.borrow().as_ref().map(|v| v.len()).unwrap_or(0) > 0);
	assert_eq!(file.expired(), false);
	file.free();
	assert_eq!(file.expired(), true);
}

#[test]
fn modified_file() {
	fn just_read(mut file: IoResult<File>) -> IoResult<Option<Vec<u8>>> {
		file.read_to_end().map(|v| Some(v))
	}

	let file = CachedProcessedFile::new(Path::new("LICENSE"), None, just_read);
	assert_eq!(file.expired(), true);
	assert!(file.borrow().as_ref().map(|v| v.len()).unwrap_or(0) > 0);
	assert_eq!(file.expired(), false);
	file.free();
	assert_eq!(file.expired(), true);
}