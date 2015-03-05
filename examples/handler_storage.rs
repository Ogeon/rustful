#![feature(io, path, fs, core)]

#[macro_use]
extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path;
use std::sync::{Arc, RwLock};
use std::error::Error;

use rustful::{Server, Context, Response, Handler, TreeRouter, Log};
use rustful::cache::{CachedValue, CachedProcessedFile};
use rustful::Method::Get;
use rustful::StatusCode::InternalServerError;
use rustful::header::ContentType;

fn main() {
    println!("Visit http://localhost:8080 to try this example.");

    //Cache the page
    let page = Arc::new(CachedProcessedFile::new(&path::Path::new("examples/handler_storage/page.html"), None, read_string));

    //The shared counter state
    let value = Arc::new(RwLock::new(0));

    let router = insert_routes!{
        TreeRouter::new() => {
            "/" => Get: Counter{
                page: page.clone(),
                value: value.clone(),
                operation: None
            },
            "/add" => Get: Counter{
                page: page.clone(),
                value: value.clone(),
                operation: Some(add as fn(i32) -> i32)
            },
            "/sub" => Get: Counter{
                page: page.clone(),
                value: value.clone(),
                operation: Some(sub as fn(i32) -> i32)
            }
        }
    };

    let server_result = Server::new().port(8080).handlers(router).run();

    match server_result {
        Ok(_server) => {},
        Err(e) => println!("could not start server: {}", e.description())
    }
}


fn add(value: i32) -> i32 {
    value + 1
}

fn sub(value: i32) -> i32 {
    value - 1
}

fn read_string(_log: &Log, file: io::Result<File>) -> io::Result<Option<String>> {
    //Read file into a string
    let mut string = String::new();
    try!(file).read_to_string(&mut string).map(|_| Some(string))
}


struct Counter<'p> {
    //We are using the handler to cache the page in this exmaple
    page: Arc<CachedProcessedFile<'p, String>>,

    value: Arc<RwLock<i32>>,
    operation: Option<fn(i32) -> i32>
}

impl<'p> Handler for Counter<'p> {
    type Cache = ();

    fn handle_request(&self, context: Context, mut response: Response) {
        self.operation.map(|op| {
            //Lock the value for writing and update it
            let mut value = self.value.write().unwrap();
            *value = op(*value);
        });

        response.set_header(ContentType(content_type!("text", "html", ("charset", "UTF-8"))));

        //Insert the value into the page and write it to the response
        match *self.page.borrow(context.log) {
            Some(ref page) => {
                let count = self.value.read().unwrap().to_string();

                if let Err(e) = response.into_writer().send(page.replace("{}", &count[..])) {
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
}