//!Response writers.

#![stable]

use std;
use std::io::{self, Write};
use std::error::Error;
use std::borrow::ToOwned;
use std::convert::From;

use hyper;
use hyper::header::{Headers, Header, HeaderFormat};
use hyper::net::Fresh;
use hyper::http::HttpWriter;
use hyper::version::HttpVersion;

use anymap::AnyMap;

use StatusCode;

use plugin::{PluginContext, ResponsePlugin};
use plugin::ResponseAction as Action;
use log::Log;

///The result of a response action.
#[unstable]
#[derive(Debug)]
pub enum ResponseError {
    ///A response plugin failed.
    PluginError(String),

    ///There was an IO error.
    IoError(io::Error)
}

impl From<io::Error> for ResponseError {
    fn from(err: io::Error) -> ResponseError {
        ResponseError::IoError(err)
    }
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ResponseError::PluginError(ref desc) => write!(f, "plugin error: {}", desc),
            ResponseError::IoError(ref e) => write!(f, "io error: {}", e)
        }
    }
}

impl Error for ResponseError {
    fn description(&self) -> &str {
        match *self {
            ResponseError::PluginError(ref desc) => desc,
            ResponseError::IoError(ref e) => Error::description(e)
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match *self {
            ResponseError::PluginError(_) => None,
            ResponseError::IoError(ref e) => Some(e)
        }
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
    writer: Option<HttpWriter<&'a mut (io::Write + 'a)>>,
    plugins: &'b Vec<Box<ResponsePlugin + Send + Sync>>,
    log: &'b (Log + 'b),
    plugin_storage: Option<AnyMap>
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
            log: log,
            plugin_storage: Some(AnyMap::new())
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

    ///Mutably borrow the plugin storage. It can be used to communicate with
    ///the response plugins.
    pub fn plugin_storage(&mut self) -> &mut AnyMap {
        self.plugin_storage.as_mut().expect("response used after drop")
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
        let mut header_result = (self.status.take().unwrap(), self.headers.take().unwrap(), Action::Write(None));

        for plugin in self.plugins {
            header_result = match header_result {
                (_, _, Action::DoNothing) => break,
                (_, _, Action::Error(_)) => break,
                (status, headers, r) => {
                    write_queue.push(r);

                    let plugin_res = {
                        let plugin_context = PluginContext {
                            storage: self.plugin_storage(),
                            log: self.log
                        };
                        plugin.begin(plugin_context, status, headers)
                    };

                    match plugin_res {
                        (status, headers, Action::Error(e)) => (status, headers, Action::Error(e)),
                        (status, headers, result) => {
                            let mut error = None;
                            
                            write_queue = write_queue.into_iter().filter_map(|action| match action {
                                Action::Write(content) => {
                                    let plugin_context = PluginContext {
                                        storage: self.plugin_storage(),
                                        log: self.log
                                    };
                                    Some(plugin.write(plugin_context, content))
                                },
                                Action::DoNothing => None,
                                Action::Error(e) => {
                                    error = Some(e);
                                    None
                                }
                            }).collect();

                            match error {
                                Some(e) => (status, headers, Action::Error(e)),
                                None => (status, headers, result)
                            }
                        }
                    }
                }
            }
        }

        let writer = match header_result {
            (_, _, Action::Error(e)) => Err(ResponseError::PluginError(e)),
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
                        (Action::Write(Some(content)), Ok(mut writer)) => match writer.write_all(content.as_bytes()) {
                            Ok(_) => Ok(writer),
                            Err(e) => Err(ResponseError::IoError(e))
                        },
                        (Action::Error(e), _) => Err(ResponseError::PluginError(e)),
                        (_, writer) => writer
                    };
                }

                writer
            }
        };

        ResponseWriter {
            writer: Some(writer),
            plugins: self.plugins,
            log: self.log,
            plugin_storage: self.plugin_storage.take().expect("response used after drop")
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
    writer: Option<Result<hyper::server::response::Response<'a, hyper::net::Streaming>, ResponseError>>,
    plugins: &'b Vec<Box<ResponsePlugin + Send + Sync>>,
    log: &'b (Log + 'b),
    plugin_storage: AnyMap
}

impl<'a, 'b> ResponseWriter<'a, 'b> {

    ///Mutably borrow the plugin storage. It can be used to communicate with
    ///the response plugins.
    pub fn plugin_storage(&mut self) -> &mut AnyMap {
        &mut self.plugin_storage
    }

    ///Writes response body data to the client.
    pub fn send<'d, Content: IntoResponseData<'d>>(&mut self, content: Content) -> Result<usize, ResponseError> {
        let mut writer = match self.writer {
            Some(Ok(ref mut writer)) => writer,
            None => return Err(ResponseError::IoError(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                return Err(e);
            } else { unreachable!(); }
        };

        let mut plugin_result = Action::write(Some(content));

        for plugin in self.plugins {
            plugin_result = match plugin_result {
                Action::Write(content) => {
                    let plugin_context = PluginContext {
                        storage: &mut self.plugin_storage,
                        log: self.log
                    };
                    plugin.write(plugin_context, content)
                },
                _ => break
            }
        }

        let write_result = match plugin_result {
            Action::Write(Some(ref s)) => {
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
            Some(Err(e)) => Err(ResponseError::IoError(e)),
            None => match plugin_result {
                Action::Error(e) => Err(ResponseError::PluginError(e)),
                Action::Write(None) => Ok(0),
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
        let mut write_queue: Vec<Action> = Vec::new();

        for plugin in self.plugins {
            let mut error = None;
            write_queue = write_queue.into_iter().filter_map(|action| match action {
                Action::Write(content) => {
                    let plugin_context = PluginContext {
                        storage: &mut self.plugin_storage,
                        log: self.log
                    };
                    Some(plugin.write(plugin_context, content))
                },
                Action::DoNothing => None,
                Action::Error(e) => {
                    error = Some(e);
                    None
                }
            }).collect();

            match error {
                Some(e) => return Err(ResponseError::PluginError(e)),
                None => {
                    let plugin_context = PluginContext {
                        storage: &mut self.plugin_storage,
                        log: self.log
                    };
                    write_queue.push(plugin.end(plugin_context))
                }
            }
        }

        for action in write_queue {
            try!{
                match action {
                    Action::Write(Some(content)) => writer.write_all(content.as_bytes()),
                    Action::Error(e) => return Err(ResponseError::PluginError(e)),
                    _ => Ok(())
                }
            }
        }

        writer.end().map_err(|e| ResponseError::IoError(e))
    }

    fn borrow_writer(&mut self) -> Result<&mut hyper::server::response::Response<'a, hyper::net::Streaming>, ResponseError> {
        match self.writer {
            Some(Ok(ref mut writer)) => Ok(writer),
            None => Err(ResponseError::IoError(io::Error::new(io::ErrorKind::BrokenPipe, "write after close"))),
            Some(Err(_)) => if let Some(Err(e)) = self.writer.take() {
                Err(e)
            } else { unreachable!(); }
        }
    }
}

impl<'a, 'b> Write for ResponseWriter<'a, 'b> {
    fn write(&mut self, content: &[u8]) -> io::Result<usize> {
        response_to_io_result(self.send(content))
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

fn response_to_io_result<T>(res:  Result<T, ResponseError>) -> io::Result<T> {
    match res {
        Ok(v) => Ok(v),
        Err(ResponseError::IoError(e)) => Err(e),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e))
    }
}