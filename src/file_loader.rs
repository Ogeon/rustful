use std::io::{self, Read};
use std::fs::File;
use std::path::Path;

use response::{Response, ResponseWriter};

pub struct FileLoader {
    ///The size, in bytes, of the file chunks. Default is 1048576 (1 megabyte).
    pub chunk_size: usize
}

impl FileLoader {
    pub fn new() -> FileLoader {
        FileLoader {
            chunk_size: 1048576
        }
    }

    pub fn send_file<'a, 'b, P: AsRef<Path>>(&self, path: P, response: Response<'a, 'b>) -> Result<(), Error<'a, 'b>> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(Error::Open(e, response))
        };

        let mut writer = response.into_writer();
        let mut buffer = vec![0; self.chunk_size];
        loop {
            match file.read(&mut buffer) {
                Ok(len) if len == 0 => break,
                Ok(len) => writer.send(&buffer[..len]),
                Err(e) => return Err(Error::Read(e, writer))
            }
        }

        Ok(())
    }
}

pub enum Error<'a, 'b> {
    Open(io::Error, Response<'a, 'b>),
    Read(io::Error, ResponseWriter<'a, 'b>)
}

impl<'a, 'b> Error<'a, 'b> {
    pub fn io_error(&self) -> &io::Error {
        match *self {
            Error::Open(ref e, _) => e,
            Error::Read(ref e, _) => e
        }
    }
}