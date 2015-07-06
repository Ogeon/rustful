//!Response writers.

use std;
use std::io::{self, Write};
use std::error;
use std::borrow::Cow;
use std::convert::From;
use std::str::{from_utf8, Utf8Error};
use std::string::{FromUtf8Error};

use hyper;
use hyper::header::{Header, HeaderFormat};

use anymap::AnyMap;

use StatusCode;

use filter::{FilterContext, ResponseFilter};
use filter::ResponseAction as Action;
use log::Log;

use Global;

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

///Unified representation of response data.
pub enum Data<'a> {
    ///Data in byte form.
    Bytes(Cow<'a, [u8]>),

    ///Data in string form.
    String(Cow<'a, str>)
}

impl<'a> Data<'a> {
    ///Borrow the content as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            &Data::Bytes(ref bytes) => bytes,
            &Data::String(ref string) => string.as_bytes(),
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
        match self {
            &Data::Bytes(ref bytes) => from_utf8(bytes),
            &Data::String(ref string) => Ok(string),
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

impl<'a> Into<Data<'a>> for Vec<u8> {
    fn into(self) -> Data<'a> {
        Data::Bytes(Cow::Owned(self))
    }
}

impl<'a> Into<Data<'a>> for &'a [u8] {
    fn into(self) -> Data<'a> {
        Data::Bytes(Cow::Borrowed(self))
    }
}

impl<'a> Into<Data<'a>> for String {
    fn into(self) -> Data<'a> {
        Data::String(Cow::Owned(self))
    }
}

impl<'a> Into<Data<'a>> for &'a str {
    fn into(self) -> Data<'a> {
        Data::String(Cow::Borrowed(self))
    }
}


///An interface for setting HTTP status code and response headers, before data gets written to the client.
pub struct Response<'a, 'b> {
    writer: Option<hyper::server::response::Response<'a>>,
    filters: &'b Vec<Box<ResponseFilter>>,
    log: &'b (Log + 'b),
    global: &'b Global,
    filter_storage: Option<AnyMap>
}

impl<'a, 'b> Response<'a, 'b> {
    pub fn new(
        response: hyper::server::response::Response<'a>,
        filters: &'b Vec<Box<ResponseFilter>>,
        log: &'b Log,
        global: &'b Global
    ) -> Response<'a, 'b> {
        Response {
            writer: Some(response),
            filters: filters,
            log: log,
            global: global,
            filter_storage: Some(AnyMap::new())
        }
    }

    ///Set HTTP status code. Ok (200) is default.
    pub fn set_status(&mut self, status: StatusCode) {
        if let Some(ref mut writer) = self.writer {
            *writer.status_mut() = status;
        }
    }

    ///Set a HTTP response header. Date, content type (text/plain) and server is automatically set.
    pub fn set_header<H: Header + HeaderFormat>(&mut self, header: H) {
        if let Some(ref mut writer) = self.writer {
            writer.headers_mut().set(header);
        }
    }

    ///Get a HTTP response header if set.
    pub fn get_header<H: Header + HeaderFormat>(&self) -> Option<&H> {
        self.writer.as_ref().and_then(|w| w.headers().get())
    }

    ///Mutably borrow the filter storage. It can be used to communicate with
    ///the response filters.
    pub fn filter_storage(&mut self) -> &mut AnyMap {
        self.filter_storage.as_mut().expect("response used after drop")
    }

    ///Turn the `Response` into a `ResponseWriter` to allow the response body to be written.
    ///
    ///Status code and headers will be written to the client and `ResponseFilter::begin()`
    ///will be called on the registered response filters.
    pub fn into_writer(mut self) -> ResponseWriter<'a, 'b> {
        self.make_writer()
    }

    fn make_writer(&mut self) -> ResponseWriter<'a, 'b> {
        let mut write_queue = Vec::new();
        let mut writer = self.writer.take().expect("response used after drop");
        let mut header_result = (writer.status(), Action::Next(None));

        for filter in self.filters {
            header_result = match header_result {
                (_, Action::SilentAbort) => break,
                (_, Action::Abort(_)) => break,
                (status, r) => {
                    write_queue.push(r);

                    let filter_res = {
                        let filter_context = FilterContext {
                            storage: self.filter_storage(),
                            log: self.log,
                            global: self.global,
                        };
                        filter.begin(filter_context, status, writer.headers_mut())
                    };

                    match filter_res {
                        (status, Action::Abort(e)) => (status, Action::Abort(e)),
                        (status, result) => {
                            let mut error = None;
                            
                            write_queue = write_queue.into_iter().filter_map(|action| match action {
                                Action::Next(content) => {
                                    let filter_context = FilterContext {
                                        storage: self.filter_storage(),
                                        log: self.log,
                                        global: self.global,
                                    };
                                    Some(filter.write(filter_context, content))
                                },
                                Action::SilentAbort => None,
                                Action::Abort(e) => {
                                    error = Some(e);
                                    None
                                }
                            }).collect();

                            match error {
                                Some(e) => (status, Action::Abort(e)),
                                None => (status, result)
                            }
                        }
                    }
                }
            }
        }

        let writer = match header_result {
            (_, Action::Abort(e)) => Err(Error::Filter(e)),
            (status, last_result) => {
                write_queue.push(last_result);

                *writer.status_mut() = status;
                let mut writer = match writer.start() {
                    Ok(writer) => Ok(writer),
                    Err(e) => Err(Error::Io(e))
                };

                for action in write_queue {
                    writer = match (action, writer) {
                        (Action::Next(Some(content)), Ok(mut writer)) => match writer.write_all(content.as_bytes()) {
                            Ok(_) => Ok(writer),
                            Err(e) => Err(Error::Io(e))
                        },
                        (Action::Abort(e), _) => Err(Error::Filter(e)),
                        (_, writer) => writer
                    };
                }

                writer
            }
        };

        ResponseWriter {
            writer: Some(writer),
            filters: self.filters,
            log: self.log,
            global: self.global,
            filter_storage: self.filter_storage.take().expect("response used after drop")
        }
    }
}

#[allow(unused_must_use)]
impl<'a, 'b> Drop for Response<'a, 'b> {
    ///Writes status code and headers and closes the connection.
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.make_writer();
        }
    }
}


///An interface for writing to the response body.
pub struct ResponseWriter<'a, 'b> {
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, Error>>,
    filters: &'b Vec<Box<ResponseFilter>>,
    log: &'b (Log + 'b),
    global: &'b Global,
    filter_storage: AnyMap
}

impl<'a, 'b> ResponseWriter<'a, 'b> {

    ///Mutably borrow the filter storage. It can be used to communicate with
    ///the response filters.
    pub fn filter_storage(&mut self) -> &mut AnyMap {
        &mut self.filter_storage
    }

    ///Write response body data to the client.
    ///
    ///Any errors that occures while writing the data will be ignored. Use
    ///`try_send`, instead, to also get error information.
    #[allow(unused_must_use)]
    pub fn send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) {
        self.try_send(content);
    }

    ///Write response body data to the client and receive the number of
    ///written bytes, or any error that occured.
    pub fn try_send<'d, Content: Into<Data<'d>>>(&mut self, content: Content) -> Result<usize, Error> {
        let mut writer = match self.writer {
            Some(Ok(ref mut writer)) => writer,
            None => return Err(Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                return Err(e);
            } else { unreachable!(); }
        };

        let mut filter_result = Action::next(Some(content));

        for filter in self.filters {
            filter_result = match filter_result {
                Action::Next(content) => {
                    let filter_context = FilterContext {
                        storage: &mut self.filter_storage,
                        log: self.log,
                        global: self.global,
                    };
                    filter.write(filter_context, content)
                },
                _ => break
            }
        }

        let write_result = match filter_result {
            Action::Next(Some(ref s)) => {
                let buf = s.as_bytes();
                match writer.write_all(buf) {
                    Ok(()) => Some(Ok(buf.len())),
                    Err(e) => Some(Err(e))
                }
            },
            _ => None
        };

        match write_result {
            Some(Ok(l)) => Ok(l),
            Some(Err(e)) => Err(Error::Io(e)),
            None => match filter_result {
                Action::Abort(e) => Err(Error::Filter(e)),
                Action::Next(None) => Ok(0),
                _ => unreachable!()
            }
        }
    }

    ///Finish writing the response and collect eventual errors.
    ///
    ///This is optional and will happen silently when the writer drops out of
    ///scope.
    pub fn end(mut self) -> Result<(), Error> {
        self.finish()
    }

    fn finish(&mut self) -> Result<(), Error> {
        let mut writer = try!(self.writer.take().expect("can only finish once"));
        let mut write_queue: Vec<Action> = Vec::new();

        for filter in self.filters {
            let mut error = None;
            write_queue = write_queue.into_iter().filter_map(|action| match action {
                Action::Next(content) => {
                    let filter_context = FilterContext {
                        storage: &mut self.filter_storage,
                        log: self.log,
                        global: self.global,
                    };
                    Some(filter.write(filter_context, content))
                },
                Action::SilentAbort => None,
                Action::Abort(e) => {
                    error = Some(e);
                    None
                }
            }).collect();

            match error {
                Some(e) => return Err(Error::Filter(e)),
                None => {
                    let filter_context = FilterContext {
                        storage: &mut self.filter_storage,
                        log: self.log,
                        global: self.global,
                    };
                    write_queue.push(filter.end(filter_context))
                }
            }
        }

        for action in write_queue {
            try!{
                match action {
                    Action::Next(Some(content)) => writer.write_all(content.as_bytes()),
                    Action::Abort(e) => return Err(Error::Filter(e)),
                    _ => Ok(())
                }
            }
        }

        writer.end().map_err(|e| Error::Io(e))
    }

    fn borrow_writer(&mut self) -> Result<&mut hyper::server::response::Response<'a, hyper::net::Streaming>, Error> {
        match self.writer {
            Some(Ok(ref mut writer)) => Ok(writer),
            None => Err(Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                Err(e)
            } else { unreachable!(); }
        }
    }
}

impl<'a, 'b> Write for ResponseWriter<'a, 'b> {
    fn write(&mut self, content: &[u8]) -> io::Result<usize> {
        response_to_io_result(self.try_send(content))
    }

    fn write_all(&mut self, content: &[u8]) -> io::Result<()> {
        self.write(content).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut writer = try!(response_to_io_result(self.borrow_writer()));
        writer.flush()
    }
}

#[allow(unused_must_use)]
impl<'a, 'b> Drop for ResponseWriter<'a, 'b> {
    ///Finishes writing and closes the connection.
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.finish();
        }
    }
}

fn response_to_io_result<T>(res:  Result<T, Error>) -> io::Result<T> {
    match res {
        Ok(v) => Ok(v),
        Err(Error::Io(e)) => Err(e),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e))
    }
}