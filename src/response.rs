//!Response writers.
//!
//!The response writers are the output channel from the handlers to the
//!client. These are used to set the response headers, as well as writing the
//!response body. Rustful provides three different types of response writers
//!with different purposes:
//!
//! * [`Response`][res] - It's used to write data with a known, fixed size,
//!that is already stored in some kind of buffer.
//! * [`Chunked`][chu] - A chunked response is a streaming response, where the final
//!size can be unknown.
//! * [`Raw`][raw] - This is also a streaming response, but with a fixed size. It is
//!unsafe to create because of the risk of sending too short responses, but it
//!can be very useful in cases where it's impractical to buffer the data, such as when
//!sending large files.
//!
//!You will always start out with a `Response`, where you can set the status
//!code and all the headers, and then transform it into one of the other
//!types, if necessary. It is usually recommended to stick to `Response` as
//!much as possible, since it has lower HTTP overhead than `Chunked` and has a
//!builtin size check that guarantees that the `content-length` field is
//!correct.
//!
//!|            | No extra overhead | Guaranteed correct `content-length` | Streaming |
//!|------------|-------------------|-------------------------------------|-----------|
//!| `Response` | &check;           | &check;                             | &cross;   |
//!| `Raw`      | &check;           | &cross;                             | &check;   |
//!| `Chunked`  | &cross;           | &check;                             | &check;   |
//!
//![res]: struct.Response.html
//![chu]: struct.Chunked.html
//![raw]: struct.Raw.html

use std;
use std::io::{self, Write};
use std::error;
use std::borrow::Cow;
use std::convert::From;
use std::str::{from_utf8, Utf8Error};
use std::string::{FromUtf8Error};

use StatusCode;

pub use interface::response::{Response, RawResponse, ResponseFilters};
pub use interface::ResponseHead;

///The result of a response action.
#[derive(Debug)]
pub enum Error {
    ///A response filter failed.
    Filter(String),

    ///There was an IO error.
    Io(io::Error)
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Filter(ref desc) => write!(f, "filter error: {}", desc),
            Error::Io(ref e) => write!(f, "io error: {}", e)
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Filter(ref desc) => desc,
            Error::Io(ref e) => e.description()
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match *self {
            Error::Filter(_) => None,
            Error::Io(ref e) => Some(e)
        }
    }
}

///Error that may occur while sending a file.
pub struct FileError {
    ///The error that occurred while reading the file.
    pub error: io::Error,

    ///The recovered HTTP response.
    pub response: Response,
}

impl FileError {
    ///Send a 404 (not found) response if the file wasn't found, or return
    ///`self` if any other error occurred.
    pub fn send_not_found<'d, M: Into<Data<'d>>>(self, message: M) -> Result<(), FileError> {
        if let io::ErrorKind::NotFound = self.error.kind() {
            let mut response = self.response;
            response.status = StatusCode::NotFound;
            response.send(message);
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl Into<io::Error> for FileError {
    fn into(self) -> io::Error {
        self.error
    }
}

impl std::fmt::Debug for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "failed to open a file: {}", self.error)
    }
}

impl error::Error for FileError {
    fn description(&self) -> &str {
        self.error.description()
    }

    fn cause(&self) -> Option<&std::error::Error> {
        Some(&self.error)
    }
}

///A unified representation of response data.
#[derive(Clone)]
pub enum Data<'a> {
    ///Data in byte form.
    Bytes(Cow<'a, [u8]>),

    ///Data in string form.
    String(Cow<'a, str>)
}

impl<'a> Data<'a> {
    ///Borrow the content as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match *self {
            Data::Bytes(ref bytes) => bytes,
            Data::String(ref string) => string.as_bytes(),
        }
    }

    ///Turns the content into a byte vector. Slices are copied.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Data::Bytes(bytes) => bytes.into_owned(),
            Data::String(string) => string.into_owned().into_bytes()
        }
    }

    ///Borrow the content as a UTF-8 string slice, if possible.
    pub fn as_string(&self) -> Result<&str, Utf8Error> {
        match *self {
            Data::Bytes(ref bytes) => from_utf8(bytes),
            Data::String(ref string) => Ok(string),
        }
    }

    ///Turn the content into a UTF-8 string, if possible. Slices are copied.
    pub fn into_string(self) -> Result<String, FromUtf8Error> {
        match self {
            Data::Bytes(bytes) => String::from_utf8(bytes.into_owned()),
            Data::String(string) => Ok(string.into_owned())
        }
    }
}

impl<'a> From<Vec<u8>> for Data<'a> {
    fn from(data: Vec<u8>) -> Data<'a> {
        Data::Bytes(Cow::Owned(data))
    }
}

impl<'a> From<&'a [u8]> for Data<'a> {
    fn from(data: &'a [u8]) -> Data<'a> {
        Data::Bytes(Cow::Borrowed(data))
    }
}

impl<'a> From<String> for Data<'a> {
    fn from(data: String) -> Data<'a> {
        Data::String(Cow::Owned(data))
    }
}

impl<'a> From<&'a str> for Data<'a> {
    fn from(data: &'a str) -> Data<'a> {
        Data::String(Cow::Borrowed(data))
    }
}

impl<'a> From<Data<'a>> for Vec<u8> {
    fn from(data: Data<'a>) -> Vec<u8> {
        data.into_bytes()
    }
}
