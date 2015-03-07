#![feature(io, path, core)]

#[macro_use]
extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path;
use std::borrow::ToOwned;
use std::error::Error;

use rustful::{Server, Context, Response, Cache, Log};
use rustful::cache::{CachedValue, CachedProcessedFile};
use rustful::context::ExtQueryBody;
use rustful::header::ContentType;
use rustful::StatusCode::{InternalServerError, BadRequest};

fn say_hello(mut context: Context<Files>, mut response: Response) {
    response.set_header(ContentType(content_type!("text", "html", ("charset", "UTF-8"))));

    let body = match context.read_query_body() {
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
            match *context.cache.form.borrow(context.log) {
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
    match *context.cache.page.borrow(context.log) {
        Some(ref page) => {
            let complete_page = page.replace("{}", &content[..]);
            if let Err(e) = response.into_writer().send(complete_page) {
	            //There is not much we can do now
	            context.log.note(&format!("could not send page: {}", e.description()));
	        }
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
        page: CachedProcessedFile::new(&path::Path::new("examples/post/page.html"), None, read_string),
        form: CachedProcessedFile::new(&path::Path::new("examples/post/form.html"), None, read_string)
    };

    //Handlers implements the Router trait, so it can be passed to the server as it is
    let server_result = Server::with_cache(cache).handlers(say_hello).port(8080).run();

    //Check if the server started successfully
    match server_result {
        Ok(_server) => {},
        Err(e) => println!("could not start server: {}", e.description())
    }
}

fn read_string(_log: &Log, file: io::Result<File>) -> io::Result<Option<String>> {
    //Read file into a string
    let mut string = String::new();
    try!(file).read_to_string(&mut string).map(|_| Some(string))
}


//We want to store the files as strings
struct Files<'p> {
    page: CachedProcessedFile<'p, String>,
    form: CachedProcessedFile<'p, String>
}

impl<'p> Cache for Files<'p> {

    //Cache cleaning is not used in this example, but this is implemented anyway.
    fn free_unused(&self, log: &Log) {
        self.page.clean(log);
        self.form.clean(log);
    }
}