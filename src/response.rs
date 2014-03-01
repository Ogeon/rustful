//! `Response` is an interface for sending HTTP response data to the client.

use http::headers::response::HeaderCollection;
use http::server::response::ResponseWriter;
pub use http::headers::content_type::MediaType;
use std::io::{Writer, IoResult};

pub use http::status;

pub struct Response<'a, 'b> {
	///The HTTP response headers. Date, content type (text/plain) and server is automatically set.
	headers: ~HeaderCollection,

	///The HTTP response status. Ok (200) is default.
	status: status::Status,
	priv writer: &'a mut ResponseWriter<'b>,
	priv started_writing: bool
}

impl<'a, 'b> Response<'a, 'b> {
	pub fn new<'a, 'b>(writer: &'a mut ResponseWriter<'b>) -> ~Response<'a, 'b> {
		~Response {
			headers: writer.headers.clone(), //Can't be borrowed, because writer must be borrowed
			status: status::Ok,
			writer: writer,
			started_writing: false
		}
	}

	///Start writing the response. Headers and status can not be changed after it has been called.
	///
	///This method will be called automatically by `write()` and `end()`, if it hasn't been called before.
	///It can only be called once.
	pub fn begin(&mut self) {
		if !self.started_writing {
			self.started_writing = true;
			//TODO: Intercept headers and status

			self.writer.status = self.status.clone();
			self.writer.headers = self.headers.clone();

			//TODO: Begin content interception
		}
	}

	///Finish writing the response.
	pub fn end(&mut self) {
		self.begin();

		//TODO: End interception
	}
}

impl<'a, 'b> Writer for Response<'a, 'b> {
	///Writes content to the client. The headers will be written the first time it's called.
	fn write(&mut self, content: &[u8]) -> IoResult<()> {
		self.begin();

		//TODO: Intercept content

		self.writer.write(content)
	}
}