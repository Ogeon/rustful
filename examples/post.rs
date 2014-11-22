#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;
use std::io::{File, IoResult};

use rustful::{Server, Request, Response, Cache};
use rustful::cache::{CachedValue, CachedProcessedFile};
use rustful::request_extensions::QueryBody;

use http::status::InternalServerError;

fn say_hello(request: Request, cache: &Files, response: &mut Response) {
	response.headers.content_type = content_type!("text", "html", "charset": "UTF-8");

	//Format the name or clone the cached form
	let content = match request.parse_query_body().get(&"name".into_string()) {
		Some(name) => {
			format!("<p>Hello, {}!</p>", name)
		},
		None => {
			match *cache.form.borrow() {
				Some(ref form) => {
					form.clone()
				},
				None => {
					//Oh no! The form was not loaded! Let's print an error message on the page.
					response.status = InternalServerError;
					"Error: Failed to load form.html".into_string()
				}
			}
		}
	};

	//Insert the content into the page and write it to the response
	match *cache.page.borrow() {
		Some(ref page) => {
			let complete_page = page.replace("{}", content.as_slice());
			try_send!(response, complete_page);
		},
		None => {
			//Oh no! The page was not loaded!
			response.status = InternalServerError;
		}
	}
	
}

fn main() {
	println!("Visit http://localhost:8080 to try this example.");

	//Fill our cache with files
	let cache = Files {
		page: CachedProcessedFile::new(Path::new("examples/post/page.html"), None, read_string),
		form: CachedProcessedFile::new(Path::new("examples/post/form.html"), None, read_string)
	};

	//Handlers implements the Router trait, so it can be passed to the server as it is
	let server = Server::with_cache(8080, cache, say_hello);

	server.run();
}

fn read_string(mut file: IoResult<File>) -> IoResult<Option<String>> {
	//Make the file mutable and try to read it into a string
	file.read_to_string().map(|s| Some(s))
}


//We want to store the files as strings
struct Files {
	page: CachedProcessedFile<String>,
	form: CachedProcessedFile<String>
}

impl Cache for Files {

	//Cache cleaning is not used in this example, but this is implemented anyway.
	fn free_unused(&self) {
		self.page.clean();
		self.form.clean();
	}
}