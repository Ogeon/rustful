#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate sync;
extern crate rustful;
extern crate http;

use std::io::{File, IoResult};
use sync::{Arc, RWLock};

use rustful::{Server, Request, Response, Handler};
use rustful::cache::{CachedValue, CachedProcessedFile};
use http::method::Get;
use http::status::InternalServerError;

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
			operation: Some(add)
		},
		"/sub" => Get: Counter{
			page: page.clone(),
			value: value.clone(),
			operation: Some(sub)
		}
	};

	let server = Server::new(8080, router);

	server.run();
}


fn add(value: int) -> int {
	value + 1
}

fn sub(value: int) -> int {
	value - 1
}

fn read_string(f: IoResult<File>) -> Option<String> {
	//Make the file mutable and try to read it into a string
	let mut file = f;
	file.read_to_string().map(|s| Some(s)).unwrap_or_else(|e| {
		println!("Unable to read file: {}", e);
		None
	})
}


struct Counter {
	//We are using the handler to cache the page in this exmaple
	page: Arc<CachedProcessedFile<String>>,

	value: Arc<RWLock<int>>,
	operation: Option<fn(int) -> int>
}

impl Handler<()> for Counter {
	fn handle_request(&self, _request: Request, _cache: &(), response: &mut Response) {
		self.operation.map(|o| {
			//Lock the value for writing and update it
			let mut value = self.value.write();
			*value = (o)(*value);
		});

		response.headers.content_type = content_type!("text", "html", "charset": "UTF-8");

		//Insert the value into the page and write it to the response
		self.page.use_value(|page| {
			match page {
				Some(page) => {
					let count = self.value.read().deref().to_string();

					try_send!(response, page.replace("{}", count.as_slice()));
				},
				None => {
					//Oh no! The page was not loaded!
					response.status = InternalServerError;
				}
			}
		});
	}
}