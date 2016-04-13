use std::io;

use hyper::net::HttpStream;
use header::Headers;
use StatusCode;

use handler::Encoder;

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
    Callback(Box<FnMut(Encoder) -> io::Result<usize> + Send>),
}
