#![feature(plugin)]

#[plugin]
#[macro_use]
#[no_link]
extern crate rustful_macros;

extern crate rustful;
use std::io::{File, IoResult};
use std::borrow::ToOwned;
use std::error::Error;

use rustful::{Server, Request, Response, Cache};
use rustful::cache::{CachedValue, CachedProcessedFile};
use rustful::request_extensions::QueryBody;
use rustful::header::ContentType;
use rustful::StatusCode::{InternalServerError, BadRequest};

fn say_hello(mut request: Request, cache: &Files, mut response: Response) {
	response.set_header(ContentType(content_type!("text", "html", ("charset", "UTF-8"))));

	let body = match request.read_query_body() {
		Ok(body) => body,
		Err(_) => {
			//Oh no! Could not read the body
			response.set_status(BadRequest);
			return;
		}
	};

	//Format the name or clone the cached form
	let content = match body.get("name") {
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
					response.set_status(InternalServerError);
					"Error: Failed to load form.html".to_owned()
				}
			}
		}
	};

	//Insert the content into the page and write it to the response
	match *cache.page.borrow() {
		Some(ref page) => {
			let complete_page = page.replace("{}", content.as_slice());
			try_send!(response.into_writer(), complete_page);
		},
		None => {
			//Oh no! The page was not loaded!
			response.set_status(InternalServerError);
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
	let server_result = Server::new().cache(cache).handlers(say_hello as fn(Request, &Files, Response)).port(8080).run();

	//Check if the server started successfully
	match server_result {
		Ok(_server) => {},
		Err(e) => println!("could not start server: {}", e.description())
	}
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