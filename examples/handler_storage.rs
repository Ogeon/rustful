#[macro_use]
extern crate rustful;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::error::Error;

use rustful::{
    Server,
    Context,
    Response,
    Handler,
    TreeRouter,
    StatusCode
};
use rustful::file::check_path;
use rustful::response::FileError;

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080 to try this example.");

    //Read the page before we start
    let page = read_string("examples/handler_storage/page.html").unwrap();

    //The shared counter state
    let value = AtomicIsize::new(0);

    //The server runs in scoped threads by default, allowing us to use
    //non-'static references in our server.
    let router = insert_routes!{
        TreeRouter::new() => {
            Get: Api::Counter {
                page: &page,
                value: &value,
                operation: None
            },
            "add" => Get: Api::Counter {
                page: &page,
                value: &value,
                operation: Some(add)
            },
            "sub" => Get: Api::Counter {
                page: &page,
                value: &value,
                operation: Some(sub)
            },
            "res/*file" => Get: Api::File
        }
    };

    let server_result = Server {
        host: 8080.into(),
        handlers: router,
        ..Server::default()
    }.run();

    if let Err(e) = server_result {
        error!("could not start server: {}", e.description())
    }
}


fn add(value: &AtomicIsize) {
    value.fetch_add(1, Ordering::SeqCst);
}

fn sub(value: &AtomicIsize) {
    value.fetch_sub(1, Ordering::SeqCst);
}

fn read_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    //Read file into a string
    let mut string = String::new();
    File::open(path).and_then(|mut f| f.read_to_string(&mut string)).map(|_| string)
}


enum Api<'env> {
    Counter {
        //We are using the handler to preload the page in this example
        page: &'env str,

        value: &'env AtomicIsize,
        operation: Option<fn(&AtomicIsize)>
    },
    File
}

impl<'env> Handler<'env> for Api<'env> {
    fn handle_request(&self, context: Context, mut response: Response) {
        match *self {
            Api::Counter { page, value, ref operation }  => {
                operation.map(|op| op(value));

                //Insert the value into the page and write it to the response
                let count = value.load(Ordering::SeqCst).to_string();
                response.send(page.replace("{}", &count));
            },
            Api::File => {
                if let Some(file) = context.variables.get("file") {
                    let file_path = Path::new(file.as_ref());

                    //Check if the path is valid
                    if check_path(file_path).is_ok() {
                        //Make a full path from the file name and send it
                        let path = Path::new("examples/handler_storage").join(file_path);
                        let res = response.send_file(path)
                            .or_else(|e| e.send_not_found("the file was not found"));

                        //Check if a more fatal file error than "not found" occurred
                        if let Err(FileError { error, mut response }) = res {
                            //Something went horribly wrong
                            error!("failed to open '{}': {}", file, error);
                            response.status = StatusCode::InternalServerError;
                        }
                    } else {
                        //Accessing parent directories is forbidden
                        response.status = StatusCode::Forbidden;
                    }
                } else {
                    //No file name was specified
                    response.status = StatusCode::Forbidden;
                }
            }
        }
    }
}
