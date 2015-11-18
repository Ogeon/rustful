#[macro_use]
extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::borrow::Cow;
use std::error::Error;

#[macro_use]
extern crate log;
extern crate env_logger;

use rustful::{Server, Context, Response};
use rustful::StatusCode::{InternalServerError, BadRequest};

fn say_hello(mut context: Context, mut response: Response) {
    let body = match context.body.read_query_body() {
        Ok(body) => body,
        Err(_) => {
            //Oh no! Could not read the body
            response.set_status(BadRequest);
            return;
        }
    };

    let files: &Files = if let Some(files) = context.global.get() {
        files
    } else {
        //Oh no! Why is the global data not a File instance?!
        error!("the global data should be of the type `Files`, but it's not");
        response.set_status(InternalServerError);
        return;
    };

    //Format the name or use the cached form
    let content = if let Some(name) = body.get("name") {
        Cow::Owned(format!("<p>Hello, {}!</p>", name))
    } else {
        Cow::Borrowed(&files.form)
    };

    //Insert the content into the page and write it to the response
    let complete_page = files.page.replace("{}", &content);
    response.send(complete_page);
}

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080 to try this example.");

    //Preload the files
    let files = Files {
        page: read_string("examples/post/page.html").unwrap(),
        form: read_string("examples/post/form.html").unwrap()
    };

    //Handlers implements the Router trait, so it can be passed to the server as it is
    let server_result = Server {
        host: 8080.into(),
        global: Box::new(files).into(),
        content_type: content_type!(Text / Html; Charset = Utf8),
        ..Server::new(say_hello)
    }.run();

    //Check if the server started successfully
    match server_result {
        Ok(_server) => {},
        Err(e) => error!("could not start server: {}", e.description())
    }
}

fn read_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    //Read file into a string
    let mut string = String::new();
    File::open(path).and_then(|mut f| f.read_to_string(&mut string)).map(|_| string)
}

//We want to store the files as strings
struct Files {
    page: String,
    form: String
}
