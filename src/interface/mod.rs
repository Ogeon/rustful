use header::Headers;
use StatusCode;

use handler::Encoder;

use self::response::RawResponse;

pub mod response;
pub mod body;

///Headers and status code of a response.
pub struct ResponseHead {
    ///The response status code.
    pub status: StatusCode,
    ///The response headers.
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
    Head(ResponseHead, ResponseType),
    Chunk(Vec<u8>),
    End,
}

pub enum ResponseType {
    Buffer(Vec<u8>),
    Callback(Box<FnMut(&mut Encoder) + Send>),
    Chunked,
}
