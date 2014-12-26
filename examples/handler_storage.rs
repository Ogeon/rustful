#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;

use std::io::{File, IoResult};
use std::sync::{Arc, RWLock};

use rustful::{Server, Request, Response, Handler};
use rustful::cache::{CachedValue, CachedProcessedFile};
use rustful::Method::Get;
use rustful::StatusCode::InternalServerError;
use rustful::header::ContentType;

fn main() {
	println!("Visit http://localhost:8080 to try this example.");

	//Cache the page
	let page = Arc::new(CachedProcessedFile::new(Path::new("examples/handler_storage/page.html"), None, read_string));

	//The shared counter state
	let value = Arc::new(RWLock::new(0));

	let router = router!{
		"/" => Get: Counter{
			page: page.clone(),
			value: value.clone(),
			operation: None
		},
		"/add" => Get: Counter{
			page: page.clone(),
			value: value.clone(),
			operation: Some(add as fn(int) -> int)
		},
		"/sub" => Get: Counter{
			page: page.clone(),
			value: value.clone(),
			operation: Some(sub as fn(int) -> int)
		}
	};

	let server_result = Server::new().port(8080).handlers(router).run();

	match server_result {
		Ok(_server) => {},
		Err(e) => println!("could not start server: {}", e)
	}
}


fn add(value: int) -> int {
	value + 1
}

fn sub(value: int) -> int {
	value - 1
}

fn read_string(mut file: IoResult<File>) -> IoResult<Option<String>> {
	//Read file into a string
	file.read_to_string().map(|s| Some(s))
}


struct Counter {
	//We are using the handler to cache the page in this exmaple
	page: Arc<CachedProcessedFile<String>>,

	value: Arc<RWLock<int>>,
	operation: Option<fn(int) -> int>
}

impl Handler<()> for Counter {
	fn handle_request(&self, _request: Request, _cache: &(), mut response: Response) {
		self.operation.map(|o| {
			//Lock the value for writing and update it
			let mut value = self.value.write();
			*value = (o)(*value);
		});

		response.set_header(ContentType(content_type!("text", "html", "charset": "UTF-8")));

		//Insert the value into the page and write it to the response
		match *self.page.borrow() {
			Some(ref page) => {
				let count = self.value.read().deref().to_string();

				try_send!(response.into_writer(), page.replace("{}", count.as_slice()));
			},
			None => {
				//Oh no! The page was not loaded!
				response.set_status(InternalServerError);
			}
		}
	}
}