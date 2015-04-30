#[macro_use]
extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::error::Error;

use rustful::{Server, Context, Response, Handler, TreeRouter, Log};
use rustful::Method::Get;
use rustful::header::ContentType;

fn main() {
    println!("Visit http://localhost:8080 to try this example.");

    //Read the page before we start
    let page = Arc::new(read_string("examples/handler_storage/page.html").unwrap());

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
                operation: Some(add)
            },
            "/sub" => Get: Counter{
                page: page.clone(),
                value: value.clone(),
                operation: Some(sub)
            }
        }
    };

    let server_result = Server {
        host: 8080.into(),
        handlers: router,
        ..Server::default()
    }.run();

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

fn read_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    //Read file into a string
    let mut string = String::new();
    File::open(path).and_then(|mut f| f.read_to_string(&mut string)).map(|_| string)
}


struct Counter {
    //We are using the handler to preload the page in this exmaple
    page: Arc<String>,

    value: Arc<RwLock<i32>>,
    operation: Option<fn(i32) -> i32>
}

impl Handler for Counter {
    fn handle_request(&self, context: Context, mut response: Response) {
        self.operation.map(|op| {
            //Lock the value for writing and update it
            let mut value = self.value.write().unwrap();
            *value = op(*value);
        });

        response.set_header(ContentType(content_type!("text", "html", ("charset", "UTF-8"))));

        //Insert the value into the page and write it to the response
        let count = self.value.read().unwrap().to_string();

        if let Err(e) = response.into_writer().send(self.page.replace("{}", &count[..])) {
            //There is not much we can do now
            context.log.note(&format!("could not send page: {}", e.description()));
        }
    }
}
