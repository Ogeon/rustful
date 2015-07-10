//!File related utilities.

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;

use response::Response;
use mime::{Mime, TopLevel, SubLevel};
use header::ContentType;

include!(concat!(env!("OUT_DIR"), "/mime.rs"));

///Returns the MIME type from a given file extension, if known.
///
///The file extension to MIME type mapping is based on [data from the Apache
///server][apache].
///
///```
///use rustful::file::ext_to_mime;
///use rustful::mime::Mime;
///use rustful::mime::TopLevel::Image;
///use rustful::mime::SubLevel::Jpeg;
///
///let mime = ext_to_mime("jpg");
///assert_eq!(mime, Some(Mime(Image, Jpeg, vec![])));
///```
///
///[apache]: http://svn.apache.org/viewvc/httpd/httpd/trunk/docs/conf/mime.types?view=markup
pub fn ext_to_mime(ext: &str) -> Option<Mime> {
    MIME.get(ext).map(|&(ref top, ref sub)| {
        Mime(top.into(), sub.into(), vec![])
    })
}

enum MaybeKnown<T> {
    Known(T),
    Unknown(&'static str)
}

impl<'a> Into<TopLevel> for &'a MaybeKnown<TopLevel> {
    fn into(self) -> TopLevel {
        match *self {
            MaybeKnown::Known(ref t) => t.clone(),
            MaybeKnown::Unknown(t) => TopLevel::Ext(t.into())
        }
    }
}

impl<'a> Into<SubLevel> for &'a MaybeKnown<SubLevel> {
    fn into(self) -> SubLevel {
        match *self {
            MaybeKnown::Known(ref s) => s.clone(),
            MaybeKnown::Unknown(s) => SubLevel::Ext(s.into())
        }
    }
}

///A utility for loading files from the file system and sending them to the
///client.
pub struct Loader {
    ///The size, in bytes, of the file chunks. Default is 1048576 (1 megabyte).
    pub buffer_size: usize
}

impl Loader {
    pub fn new() -> Loader {
        Loader {
            buffer_size: 1048576
        }
    }

    ///Send a file to the client.
    ///
    ///A MIME type is automatically applied to the response, based on the file
    ///extension, and `application/octet-stream` is used as a fallback if the
    ///extension is unknown. See [`ext_to_mime`](fn.ext_to_mime.html) for more
    ///information.
    pub fn send_file<'a, 'b, P: AsRef<Path>>(&self, path: P, mut response: Response<'a, 'b>) -> Result<(), Error<'a, 'b>> {
        let path: &Path = path.as_ref();
        let mime = path
            .extension()
            .and_then(|ext| ext_to_mime(&ext.to_string_lossy()))
            .unwrap_or(Mime(TopLevel::Application, SubLevel::Ext("octet-stream".into()), vec![]));

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(Error::Open(e, response))
        };
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(e) => return Err(Error::Open(e, response))
        };

        response.headers_mut().set(ContentType(mime));

        let mut writer = unsafe { response.into_raw(metadata.len()) };
        let mut buffer = vec![0; self.buffer_size];
        loop {
            match file.read(&mut buffer) {
                Ok(len) if len == 0 => break,
                Ok(len) => try!(writer.try_send(&buffer[..len]).map_err(|e| Error::Transfer(e))),
                Err(e) => return Err(Error::Read(e))
            }
        }

        Ok(())
    }
}

///Error types from `Loader`.
pub enum Error<'a, 'b> {
    ///Failed to open the file.
    Open(io::Error, Response<'a, 'b>),
    ///Failed while reading the file.
    Read(io::Error),
    ///Failed while trasferring the file.
    Transfer(io::Error)
}

impl<'a, 'b> Error<'a, 'b> {
    pub fn io_error(&self) -> &io::Error {
        match *self {
            Error::Open(ref e, _) => e,
            Error::Read(ref e) => e,
            Error::Transfer(ref e) => e
        }
    }
}
