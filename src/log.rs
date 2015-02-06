//!Log tools.

use std::old_io::{self, Writer};
use std::sync::Mutex;

///Common trait for log tools.
pub trait Log {
	///Print a note to the log.
	fn note(&self, message: &str);
	///Print a warning to the log.
	fn warning(&self, message: &str);
	///Print an error to the log.
	fn error(&self, message: &str);
}

///Log tool for printing to standard output.
pub struct StdOut;

impl Log for StdOut {
	fn note(&self, message: &str) {
		println!("note: {}", message);
	}

	fn warning(&self, message: &str) {
		println!("warning: {}", message);
	}

	fn error(&self, message: &str) {
		println!("error: {}", message);
	}
}

///Log tool for printing to a file.
pub struct File {
	file: Mutex<old_io::File>
}

impl File {
	pub fn new(file: old_io::File) -> File {
		File {
			file: Mutex::new(file)
		}
	}
}

impl Log for File {
	fn note(&self, message: &str) {
		let mut f = match self.file.lock() {
			Ok(f) => f,
			Err(e) => e.into_inner()
		};
		write!(f, "note: {}", message);
	}

	fn warning(&self, message: &str) {
		let mut f = match self.file.lock() {
			Ok(f) => f,
			Err(e) => e.into_inner()
		};
		write!(f, "warning: {}", message);
	}

	fn error(&self, message: &str) {
		let mut f = match self.file.lock() {
			Ok(f) => f,
			Err(e) => e.into_inner()
		};
		write!(f, "error: {}", message);
	}
}

#[cfg(test)]
mod test {
	use std::old_io::{self, TempDir};
	use log;
	use Server;
	use Context;
	use Response;

	fn handler(_c: Context, _w: Response) {}

	#[test]
	fn log_to_file() {
		let dir = TempDir::new("log_to_file").unwrap();
		let file = old_io::File::create(&dir.path().join("test.log")).unwrap();
		Server::new().handlers(handler).log(log::File::new(file)).build();
	}
}