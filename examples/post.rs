extern crate rustful;

use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::borrow::Cow;
use std::error::Error as ErrorTrait;

#[macro_use]
extern crate log;
extern crate env_logger;

use rustful::{Server, Context, Response, SendResponse};
use rustful::StatusCode::{InternalServerError, BadRequest};

fn say_hello(context: &mut Context) -> Result<String, Error> {
    let body = context.body.read_query_body().map_err(|_| Error::CouldNotReadBody)?;
    let files: &Files = context.global.get().ok_or(Error::MissingFileCache)?;

    //Format the name or use the cached form
    let content = if let Some(name) = body.get("name") {
        Cow::Owned(format!("<p>Hello, {}!</p>", name))
    } else {
        Cow::Borrowed(&files.form)
    };

    //Insert the content into the page and write it to the response
    Ok(files.page.replace("{}", &content))
}

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080 to try this example.");

    //Preload the files
    let files = Files {
        page: read_string("examples/post/page.html").unwrap(),
        form: read_string("examples/post/form.html").unwrap()
    };

    let server_result = Server {
        host: 8080.into(),
        global: Box::new(files).into(),
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

#[derive(PartialEq)]
enum Error {
    CouldNotReadBody,
    MissingFileCache
}

impl SendResponse for Error {
    type Error = rustful::Error;

    fn prepare_response(&mut self, response: &mut Response) {
        match *self {
            Error::CouldNotReadBody => response.set_status(BadRequest),
            Error::MissingFileCache => response.set_status(InternalServerError),
        }
    }

    fn send_response<'a, 'b>(self, response: Response<'a, 'b>) -> Result<(), (Option<Response<'a, 'b>>, rustful::Error)> {
        if self == Error::MissingFileCache {
            error!("the global data should be of the type `Files`, but it's not");
        }

        response.try_send("")
    }
}
