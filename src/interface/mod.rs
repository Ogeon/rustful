use std::io;

use hyper::Encoder;
use hyper::net::HttpStream;
use header::Headers;
use StatusCode;

use self::response::RawResponse;

pub mod response;

pub struct ResponseHead {
    pub status: StatusCode,
    pub headers: Headers,
}

impl From<RawResponse> for ResponseHead {
	fn from(response: RawResponse) -> ResponseHead {
		ResponseHead {
			status: response.status,
			headers: response.headers,
		}
	}
}

pub enum ResponseMessage {
    Head(ResponseHead),
    Buffer(Vec<u8>),
    Callback(Box<FnMut(&mut Encoder<HttpStream>) -> io::Result<usize> + Send>),
}
