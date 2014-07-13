#![feature(phase)]
#[phase(plugin)]
extern crate rustful_macros;

extern crate rustful;
extern crate http;
use std::io::{File, IoResult};
use std::os::{self_exe_path, getcwd};

use rustful::{Server, Request, Response, Cache};
use rustful::cache::{CachedValue, CachedProcessedFile};
use http::method::{Get, Post};
use http::status::InternalServerError;

fn say_hello(request: Request, cache: &Files, response: &mut Response) {
	response.headers.content_type = content_type!("text", "html", "charset": "UTF-8");

	//Format the name or clone the cached form
	let content = match request.post.find(&"name".into_string()) {
		Some(name) => {
			format!("<p>Hello, {}!</p>", name)
		},
		None => {
			cache.form.use_value(|form| {
				match form {
					Some(form) => {
						form.clone()
					},
					None => {
						//Oh no! The form was not loaded! Let's print an error message on the page.
						response.status = InternalServerError;
						"Error: Failed to load form.html".into_string()
					}
				}
			})
		}
	};

	//Insert the content into the page and write it to the response
	cache.page.use_value(|page| {
		match page {
			Some(page) => {
				let complete_page = page.replace("{}", content.as_slice());
				try_send!(response, complete_page);
			},
			None => {
				//Oh no! The page was not loaded!
				response.status = InternalServerError;
			}
		}
	});
	
}

fn main() {
	println!("Visit http://localhost:8080 to try this example.");

	//Get the directory of the example or fall back to the current working directory
	let base_path = self_exe_path().unwrap_or_else(|| getcwd());

	//Fill our cache with files
	let cache = Files {
		page: CachedProcessedFile::new(base_path.join("page.html"), None, read_string),
		form: CachedProcessedFile::new(base_path.join("form.html"), None, read_string)
	};

	let server = Server::with_cache(8080, cache, router!{"/" => Get | Post: say_hello});

	server.run();
}

fn read_string(f: IoResult<File>) -> Option<String> {
	//Make the file mutable and try to read it into a string
	let mut file = f;
	file.read_to_string().map(|s| Some(s)).unwrap_or_else(|e| {
		println!("Unable to read file: {}", e);
		None
	})
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