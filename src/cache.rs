//!Utility traits and implementations for cached resources.

use std::io::{File, IoResult};
use std::io::fs::PathExtensions;

use time;
use time::Timespec;

use sync::RWLock;

///This trait provides functions for handling cached resources.
pub trait CachedValue<T> {
	///`do_this` with the cached value, without loading or reloading it.
	fn use_current_value<R>(&self, do_this: |Option<&T>| -> R) -> R;

	///Load the cached value.
	fn load(&self);

	///Free the cached value.
	fn free(&self);

	///Check if the cached value has expired.
	fn expired(&self) -> bool;

	///Check if the cached value is unused and should be removed.
	fn unused(&self) -> bool;

	///Reload the cached value if it has expired and `do_this` with it.
	fn use_value<R>(&self, do_this: |Option<&T>| -> R) -> R {
		if self.expired() {
			self.load();
		}

		self.use_current_value(do_this)
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
pub struct CachedFile {
	path: Path,
	file: RWLock<Option<Vec<u8>>>,
	modified: RWLock<u64>,
	last_accessed: RWLock<Timespec>,
	unused_after: Option<i64>
}

impl CachedFile {
	///Creates a new `CachedFile` which will be freed `unused_after` seconds after the latest access.
	pub fn new(path: Path, unused_after: Option<u32>) -> CachedFile {
		CachedFile {
			path: path,
			file: RWLock::new(None),
			modified: RWLock::new(0),
			last_accessed: RWLock::new(Timespec::new(0, 0)),
			unused_after: unused_after.map(|i| i as i64),
		}
	}
}

impl CachedValue<Vec<u8>> for CachedFile {
	fn use_current_value<R>(&self, do_this: |Option<&Vec<u8>>| -> R) -> R {
		if self.unused_after.is_some() {
			*self.last_accessed.write() = time::get_time();
		}
		
		do_this(self.file.read().as_ref())
	}

	fn load(&self) {
		*self.modified.write() = self.path.stat().map(|s| s.modified).unwrap_or(0);
		*self.file.write() = File::open(&self.path).read_to_end().map(|v| Some(v)).unwrap_or(None);

		if self.unused_after.is_some() {
			*self.last_accessed.write() = time::get_time();
		}
	}

	fn free(&self) {
		*self.file.write() = None;
	}

	fn expired(&self) -> bool {
		if self.file.read().is_some() {
			self.path.stat().map(|s| s.modified > *self.modified.read()).unwrap_or(false)
		} else {
			true
		}
	}

	fn unused(&self) -> bool {
		if self.file.read().is_some() {
			self.unused_after.map(|t| {
				let last_accessed = self.last_accessed.read();
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
///The file will be processed by a provided function when loaded and the result will be stored.
pub struct CachedProcessedFile<T> {
	path: Path,
	file: RWLock<Option<T>>,
	modified: RWLock<u64>,
	last_accessed: RWLock<Timespec>,
	unused_after: Option<i64>,
	processor: fn(IoResult<File>) -> Option<T>
}

impl<T: Send+Sync> CachedProcessedFile<T> {
	///Creates a new `CachedProcessedFile` which will be freed `unused_after` seconds after the latest access.
	///The file will be processed by the provided `processor` function each time it's loaded.
	pub fn new(path: Path, unused_after: Option<u32>, processor: fn(IoResult<File>) -> Option<T>) -> CachedProcessedFile<T> {
		CachedProcessedFile {
			path: path,
			file: RWLock::new(None),
			modified: RWLock::new(0),
			last_accessed: RWLock::new(Timespec::new(0, 0)),
			unused_after: unused_after.map(|i| i as i64),
			processor: processor
		}
	}
}

impl<T: Send+Sync> CachedValue<T> for CachedProcessedFile<T> {
	fn use_current_value<R>(&self, do_this: |Option<&T>| -> R) -> R {
		if self.unused_after.is_some() {
			*self.last_accessed.write() = time::get_time();
		}

		do_this(self.file.read().as_ref())
	}

	fn load(&self) {
		*self.modified.write() = self.path.stat().map(|s| s.modified).unwrap_or(0);
		*self.file.write() = (self.processor)(File::open(&self.path));

		if self.unused_after.is_some() {
			*self.last_accessed.write() = time::get_time();
		}
	}

	fn free(&self) {
		*self.file.write() = None;
	}

	fn expired(&self) -> bool {
		if self.file.read().is_some() {
			self.path.stat().map(|s| s.modified > *self.modified.read()).unwrap_or(true)
		} else {
			true
		}
	}

	fn unused(&self) -> bool {
		if self.file.read().is_some() {
			self.unused_after.map(|t| {
				let last_accessed = self.last_accessed.read();
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
	assert!(file.use_value(|o| o.map(|v| v.len()).unwrap_or(0)) > 0);
	assert_eq!(file.expired(), false);
	file.free();
	assert_eq!(file.expired(), true);
}

#[test]
fn modified_file() {
	fn just_read(f: IoResult<File>) -> Option<Vec<u8>> {
		let mut file = f;
		file.read_to_end().map(|v| Some(v)).unwrap_or(None)
	}

	let file = CachedProcessedFile::new(Path::new("LICENSE"), None, just_read);
	assert_eq!(file.expired(), true);
	assert!(file.use_value(|o| o.map(|v| v.len()).unwrap_or(0)) > 0);
	assert_eq!(file.expired(), false);
	file.free();
	assert_eq!(file.expired(), true);
}