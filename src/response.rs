//!Response writers.

#![stable]

use std::old_io::{IoResult, IoError, Writer, OtherIoError};
use std::error::FromError;
use std::borrow::ToOwned;

use hyper;
use hyper::header::{Headers, Header, HeaderFormat};
use hyper::net::Fresh;
use hyper::http::HttpWriter;
use hyper::version::HttpVersion;

use StatusCode;

use plugin::ResponsePlugin;
use plugin::ResponseAction::{self, Write, DoNothing, Error};
use log::Log;

///The result of a response action.
#[unstable]
#[derive(Clone)]
pub enum ResponseError {
    ///A response plugin failed.
    PluginError(String),

    ///There was an IO error.
    IoError(IoError)
}

impl FromError<IoError> for ResponseError {
    fn from_error(err: IoError) -> ResponseError {
        ResponseError::IoError(err)
    }
}

#[stable]
pub enum ResponseData<'a> {
    ///Data in byte form.
    #[stable]
    Bytes(Vec<u8>),

    ///Data in byte form.
    #[stable]
    ByteSlice(&'a [u8]),

    ///Data in string form.
    #[stable]
    String(String),

    ///Data in string form.
    #[stable]
    StringSlice(&'a str)
}

#[stable]
impl<'a> ResponseData<'a> {
    ///Borrow the content as a byte slice.
    #[stable]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            &ResponseData::Bytes(ref bytes) => bytes,
            &ResponseData::ByteSlice(ref bytes) => bytes,
            &ResponseData::String(ref string) => string.as_bytes(),
            &ResponseData::StringSlice(ref string) => string.as_bytes()
        }
    }

    ///Turns the content into a byte vector. Slices are copied.
    #[stable]
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            ResponseData::Bytes(bytes) => bytes,
            ResponseData::ByteSlice(bytes) => bytes.to_vec(),
            ResponseData::String(string) => string.into_bytes(),
            ResponseData::StringSlice(string) => string.as_bytes().to_vec()
        }
    }

    ///Borrow the content as a string slice if the content is a string.
    ///Returns an `None` if the content is a byte vector, a byte slice or if the action is `Error`.
    #[stable]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            &ResponseData::String(ref string) => Some(string),
            &ResponseData::StringSlice(ref string) => Some(string),
            _ => None
        }
    }

    ///Extract the contained string or string slice if there is any.
    ///Returns an `None` if the content is a byte vector, a byte slice or if the action is `Error`.
    ///Slices are copied.
    #[unstable = "may change to use Cow"]
    pub fn into_string(self) -> Option<String> {
        match self {
            ResponseData::String(string) => Some(string),
            ResponseData::StringSlice(string) => Some(string.to_owned()),
            _ => None
        }
    }
}


///Represents anything that can be turned into `ResponseData`.
#[stable]
pub trait IntoResponseData<'a> {
    #[stable]
    fn into_response_data(self) -> ResponseData<'a>;
}

impl IntoResponseData<'static> for Vec<u8> {
    fn into_response_data(self) -> ResponseData<'static> {
        ResponseData::Bytes(self)
    }
}

impl<'a> IntoResponseData<'a> for &'a [u8] {
    fn into_response_data(self) -> ResponseData<'a> {
        ResponseData::ByteSlice(self)
    }
}

impl IntoResponseData<'static> for String {
    fn into_response_data(self) -> ResponseData<'static> {
        ResponseData::String(self)
    }
}

impl<'a> IntoResponseData<'a> for &'a str {
    fn into_response_data(self) -> ResponseData<'a> {
        ResponseData::StringSlice(self)
    }
}

impl<'a> IntoResponseData<'a> for ResponseData<'a> {
    fn into_response_data(self) -> ResponseData<'a> {
        self
    }
}


///An interface for setting HTTP status code and response headers, before data gets written to the client.
pub struct Response<'a, 'b> {
    headers: Option<Headers>,

    status: Option<StatusCode>,

    version: Option<HttpVersion>,
    writer: Option<HttpWriter<&'a mut (Writer + 'a)>>,
    plugins: &'b Vec<Box<ResponsePlugin + Send + Sync>>,
    log: &'b (Log + 'b)
}

impl<'a, 'b> Response<'a, 'b> {
    pub fn new(response: hyper::server::response::Response<'a>, plugins: &'b Vec<Box<ResponsePlugin + Send + Sync>>, log: &'b Log) -> Response<'a, 'b> {
        let (version, writer, status, headers) = response.deconstruct();
        Response {
            headers: Some(headers),
            status: Some(status),
            version: Some(version),
            writer: Some(writer),
            plugins: plugins,
            log: log
        }
    }

    ///Set HTTP status code. Ok (200) is default.
    pub fn set_status(&mut self, status: StatusCode) {
        self.status = Some(status);
    }

    ///Set a HTTP response header. Date, content type (text/plain) and server is automatically set.
    pub fn set_header<H: Header + HeaderFormat>(&mut self, header: H) {
        if let Some(ref mut headers) = self.headers {
            headers.set(header);
        }
    }

    ///Get a HTTP response header if set.
    pub fn get_header<H: Header + HeaderFormat>(&self) -> Option<&H> {
        self.headers.as_ref().and_then(|h| h.get::<H>())
    }

    ///Turn the `Response` into a `ResponseWriter` to allow the response body to be written.
    ///
    ///Status code and headers will be written to the client and `ResponsePlugin::begin()`
    ///will be called on the registered response plugins.
    pub fn into_writer(mut self) -> ResponseWriter<'a, 'b> {
        self.make_writer()
    }

    fn make_writer(&mut self) -> ResponseWriter<'a, 'b> {
        let mut write_queue = Vec::new();
        let mut header_result = (self.status.take().unwrap(), self.headers.take().unwrap(), Write(None));

        for plugin in self.plugins {
            header_result = match header_result {
                (_, _, DoNothing) => break,
                (_, _, Error(_)) => break,
                (status, headers, r) => {
                    write_queue.push(r);

                    match plugin.begin(self.log, status, headers) {
                        (status, headers, Error(e)) => (status, headers, Error(e)),
                        (status, headers, result) => {
                            let mut error = None;
                            
                            write_queue = write_queue.into_iter().filter_map(|action| match action {
                                Write(content) => Some(plugin.write(self.log, content)),
                                DoNothing => None,
                                Error(e) => {
                                    error = Some(e);
                                    None
                                }
                            }).collect();

                            match error {
                                Some(e) => (status, headers, Error(e)),
                                None => (status, headers, result)
                            }
                        }
                    }
                }
            }
        }

        let writer = match header_result {
            (_, _, Error(e)) => Err(ResponseError::PluginError(e)),
            (status, headers, last_result) => {
                write_queue.push(last_result);

                let version = self.version.take().unwrap();
                let writer = self.writer.take().unwrap();
                let writer = hyper::server::response::Response::<Fresh>::construct(version, writer, status, headers).start();
                let mut writer = match writer {
                    Ok(writer) => Ok(writer),
                    Err(e) => Err(ResponseError::IoError(e))
                };

                for action in write_queue {
                    writer = match (action, writer) {
                        (Write(Some(content)), Ok(mut writer)) => match writer.write_all(content.as_bytes()) {
                            Ok(_) => Ok(writer),
                            Err(e) => Err(ResponseError::IoError(e))
                        },
                        (Error(e), _) => Err(ResponseError::PluginError(e)),
                        (_, writer) => writer
                    };
                }

                writer
            }
        };

        ResponseWriter {
            writer: Some(writer),
            plugins: self.plugins,
            log: self.log
        }
    }
}

#[unsafe_destructor]
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
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, ResponseError>>,
    plugins: &'b Vec<Box<ResponsePlugin + Send + Sync>>,
    log: &'b (Log + 'b)
}

impl<'a, 'b> ResponseWriter<'a, 'b> {

    ///Writes response body data to the client.
    pub fn send<'d, Content: IntoResponseData<'d>>(&mut self, content: Content) -> Result<(), ResponseError> {
        let mut writer = try!(self.writer.as_mut().expect("write after close").as_mut().map_err(|e| e.clone()));
        let mut plugin_result = ResponseAction::write(Some(content));

        for plugin in self.plugins {
            plugin_result = match plugin_result {
                Write(content) => plugin.write(self.log, content),
                _ => break
            }
        }

        let write_result = match plugin_result {
            Write(Some(ref s)) => Some(writer.write_all(s.as_bytes())),
            _ => None
        };

        match write_result {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(ResponseError::IoError(e)),
            None => match plugin_result {
                Error(e) => Err(ResponseError::PluginError(e)),
                Write(None) => Ok(()),
                _ => unreachable!()
            }
        }
    }

    ///Finish writing the response and collect eventual errors.
    ///
    ///This is optional and will happen when the writer drops out of scope.
    pub fn end(mut self) -> Result<(), ResponseError> {
        self.finish()
    }

    fn finish(&mut self) -> Result<(), ResponseError> {
        let mut writer = try!(self.writer.take().expect("can only finish once"));
        let mut write_queue: Vec<ResponseAction> = Vec::new();

        for plugin in self.plugins {
            let mut error = None;
            write_queue = write_queue.into_iter().filter_map(|action| match action {
                Write(content) => Some(plugin.write(self.log, content)),
                DoNothing => None,
                Error(e) => {
                    error = Some(e);
                    None
                }
            }).collect();

            match error {
                Some(e) => return Err(ResponseError::PluginError(e)),
                None => write_queue.push(plugin.end(self.log))
            }
        }

        for action in write_queue {
            try!{
                match action {
                    Write(Some(content)) => writer.write_all(content.as_bytes()),
                    Error(e) => return Err(ResponseError::PluginError(e)),
                    _ => Ok(())
                }
            }
        }

        writer.end().map_err(|e| ResponseError::IoError(e))
    }
}

impl<'a, 'b> Writer for ResponseWriter<'a, 'b> {
    fn write_all(&mut self, content: &[u8]) -> IoResult<()> {
        match self.send(content) {
            Ok(()) => Ok(()),
            Err(ResponseError::IoError(e)) => Err(e),
            Err(ResponseError::PluginError(e)) => Err(IoError{
                kind: OtherIoError,
                desc: "response plugin error",
                detail: Some(e)
            })
        }
    }
}

#[unsafe_destructor]
#[allow(unused_must_use)]
impl<'a, 'b> Drop for ResponseWriter<'a, 'b> {
    ///Finishes writing and closes the connection.
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.finish();
        }
    }
}