#[macro_use]
extern crate rustful;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate tempdir;

use std::error::Error;
use std::io::{self, Read};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use tempdir::TempDir;

use rustful::{Server, Context, Response, Handler, StatusCode};
use rustful::router::TreeRouter;
use rustful::mime::TopLevel;
use rustful::file::check_path;
use rustful::response::FileError;

fn main() {
    env_logger::init().unwrap();

    //Read the pages before we start.
    let form = read_string("examples/multipart/form.html").unwrap();
    let image = read_string("examples/multipart/image.html").unwrap();
    let error = read_string("examples/multipart/error.html").unwrap();

    //Create a temporary image directory.
    let image_dir = Arc::new(tempdir::TempDir::new("rustful_multipart").unwrap());

    let router = insert_routes! {
        TreeRouter::new() => {
            Get: Api {
                image_dir: image_dir.clone(),
                page: ApiPage::Form(form),
            },
            Post: Api {
                image_dir: image_dir.clone(),
                page: ApiPage::Display { on_ok: Arc::new(image), on_err: Arc::new(error) },
            },
            "img/*file" => Get: Api {
                image_dir: image_dir,
                page: ApiPage::File,
            },
        }
    };

    let server_result = Server {
        handlers: router,
        host: 8080.into(),
        content_type: content_type!(Text / Html; Charset = Utf8),
        ..Server::default()
    }.run();

    if let Err(e) = server_result {
        error!("could not start server: {}", e.description())
    } else {
        println!("Visit http://localhost:8080 to try this example.");
    }
}

fn read_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    //Read file into a string
    let mut string = String::new();
    File::open(path).and_then(|mut f| f.read_to_string(&mut string)).map(|_| string)
}

struct Api {
    image_dir: Arc<TempDir>,
    page: ApiPage,
}

enum ApiPage {
    Form(String),
    Display {
        on_ok: Arc<String>,
        on_err: Arc<String>,
    },
    File,
}

impl Handler for Api {
    fn handle_request(&self, context: Context, mut response: Response) {
        match self.page {
            ApiPage::Form(ref form) => response.send(&**form),
            ApiPage::Display { ref on_ok, ref on_err } => {
                //Multipart request can't be read asynchronously, at the
                //moment, so we have to do it in another thread.
                let on_ok = on_ok.clone();
                let on_err = on_err.clone();
                let image_dir = self.image_dir.clone();

                context.body.sync_read(move |body| {
                    if let Ok(mut multipart) = body.into_multipart() {
                        let mut caption = None;
                        let mut file = None;

                        //Iterate through the form fields and read those we need.
                        let res = multipart.foreach_entry(|mut entry| match &*entry.name {
                            "caption" => caption = entry.data.as_text().map(ToOwned::to_owned),
                            "image" => file = entry.data.as_file().and_then(|f| f.save_in(image_dir.path()).ok().map(|file| (file, f.content_type().0.clone()))),
                            _ => {},
                        });

                        if res.is_ok() {
                            match (caption, file) {
                                (None, _) => {
                                    response.status = StatusCode::BadRequest;
                                    response.send("missing caption");
                                },
                                (_, None) => {
                                    response.status = StatusCode::BadRequest;
                                    response.send("missing image file");
                                },
                                (Some(caption), Some((file, content_type))) => {
                                    //Make a really lousy check to see that
                                    //the uploaded file was an image.
                                    if content_type == TopLevel::Image {
                                        //Format and send the page.
                                        response.send(on_ok
                                            .replace("{{caption}}", &caption)
                                            .replace("{{src}}", &format!("/img/{}", file.path.file_name().expect("the image has no name").to_string_lossy()))
                                        );
                                    } else {
                                        response.send(&**on_err);
                                    }
                                }
                            }
                        } else {
                            response.status = StatusCode::BadRequest;
                            response.send("failed to parse multipart request");
                        }
                    } else {
                        response.status = StatusCode::BadRequest;
                        response.send("expected multipart encoding");
                    }
                });
            },
            ApiPage::File => {
                if let Some(file) = context.variables.get("file") {
                    let file_path = Path::new(file.as_ref());

                    //Check if the path is valid
                    if check_path(file_path).is_ok() {
                        //Make a full path from the file name and send it
                        let path = self.image_dir.path().join(file_path);
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
