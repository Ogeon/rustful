#[macro_use]
extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::error::Error;

use rustful::{
    Server,
    Context,
    Response,
    Handler,
    TreeRouter,
    StatusCode
};
use rustful::file::{self, Loader};

fn main() {
    println!("Visit http://localhost:8080 to try this example.");

    //Read the page before we start
    let page = Arc::new(read_string("examples/handler_storage/page.html").unwrap());

    //The shared counter state
    let value = Arc::new(RwLock::new(0));

    let router = insert_routes!{
        TreeRouter::new() => {
            Get: Api::Counter {
                page: page.clone(),
                value: value.clone(),
                operation: None
            },
            "add" => Get: Api::Counter {
                page: page.clone(),
                value: value.clone(),
                operation: Some(add)
            },
            "sub" => Get: Api::Counter {
                page: page.clone(),
                value: value.clone(),
                operation: Some(sub)
            },
            "res/:file" => Get: Api::File
        }
    };

    let server_result = Server {
        host: 8080.into(),
        handlers: router,
        content_type: content_type!(Text / Html; Charset = Utf8),
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


enum Api {
    Counter {
        //We are using the handler to preload the page in this exmaple
        page: Arc<String>,

        value: Arc<RwLock<i32>>,
        operation: Option<fn(i32) -> i32>
    },
    File
}

impl Handler for Api {
    fn handle_request(&self, context: Context, mut response: Response) {
        match *self {
            Api::Counter { ref page, ref value, ref operation }  => {
                operation.map(|op| {
                    //Lock the value for writing and update it
                    let mut value = value.write().unwrap();
                    *value = op(*value);
                });

                //Insert the value into the page and write it to the response
                let count = value.read().unwrap().to_string();
                response.send_only(page.replace("{}", &count[..]));
            },
            Api::File => {
                if let Some(file) = context.variables.get("file") {
                    //Make a full path from the file name and send it
                    let path = format!("examples/handler_storage/{}", file);
                    let res = Loader::new().send_file(&path, response);

                    //Check if file could be opened
                    if let Err(file::Error::Open(e, mut response)) = res {
                        if let io::ErrorKind::NotFound = e.kind() {
                            response.set_status(StatusCode::NotFound);
                        } else {
                            //Something went horribly wrong
                            context.log.error(&format!("failed to open '{}': {}", file, e.description()));
                            response.set_status(StatusCode::InternalServerError);
                        }
                    }
                } else {
                    //No file name was specified
                    response.set_status(StatusCode::Forbidden);
                }
            }
        }
    }
}
