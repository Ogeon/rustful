#[macro_use]
extern crate rustful;
extern crate rustc_serialize;
extern crate unicase;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::sync::RwLock;
use std::collections::btree_map::{BTreeMap, Iter};

use rustc_serialize::json;
use unicase::UniCase;

use rustful::{
    Server,
    Context,
    Response,
    Handler,
    TreeRouter
};
use rustful::header::{
    ContentType,
    AccessControlAllowOrigin,
    AccessControlAllowMethods,
    AccessControlAllowHeaders,
    Host
};
use rustful::StatusCode;
use rustful::response::SendResponse;

fn main() {
    env_logger::init().unwrap();

    let mut router = insert_routes!{
        TreeRouter::new() => {
            Get: Api(Some(list_all)),
            Post: Api(Some(store)),
            Delete: Api(Some(clear)),
            Options: Api(None),
            ":id" => {
                Get: Api(Some(get_todo)),
                Patch: Api(Some(edit_todo)),
                Delete: Api(Some(delete_todo)),
                Options: Api(None)
            }
        }
    };

    //Enables hyperlink search, which will be used in CORS
    router.find_hyperlinks = true;

    //Our imitation of a database
    let database = RwLock::new(Table::new());

    let server_result = Server {
        handlers: router,
        host: 8080.into(),
        content_type: content_type!(Application / Json; Charset = Utf8),
        global: Box::new(database).into(),
        ..Server::default()
    }.run();

    match server_result {
      Ok(server) => {
        println!(
          "This example is a showcase implementation of the Todo-Backend project (http://todobackend.com/), \
          visit http://localhost:{0}/ to try it or run reference test suite by pointing \
          your browser to http://todobackend.com/specs/index.html?http://localhost:{0}",
          server.socket.port()
        );
      },
      Err(e) => error!("could not run the server: {}", e)
    };
}

//Errors that may occur while parsing the request
enum Error {
    ParseError,
    BadId,
    MissingHostHeader,
}

impl<'a, 'b> SendResponse<'a, 'b> for Error {
    type Error = rustful::Error;

    fn send_response(self, mut response: Response<'a, 'b>) -> Result<(), rustful::Error> {
        let message = match self {
            Error::ParseError => "Couldn't parse the todo",
            Error::BadId => "The 'id' parameter should be a non-negative integer",
            Error::MissingHostHeader => "No 'Host' header was sent",
        };

        response.headers_mut().set(ContentType(content_type!(Text / Plain; Charset = Utf8)));
        response.set_status(StatusCode::BadRequest);
        message.send_response(response)
    }
}



//List all the to-dos in the database
fn list_all(database: &Database, context: Context) -> Result<Option<String>, Error> {
    let host = try!(context.headers.get().ok_or(Error::MissingHostHeader));

    let todos: Vec<_> = database.read().unwrap().iter()
      .map(|(&id, todo)| NetworkTodo::from_todo(todo, host, id))
      .collect();

    Ok(Some(json::encode(&todos).unwrap()))
}

//Store a new to-do with data from the request body
fn store(database: &Database, mut context: Context) -> Result<Option<String>, Error> {
    let todo: NetworkTodo = try!(
        context.body
        .decode_json_body()
        .map_err(|_| Error::ParseError)
    );

    let host = try!(context.headers.get().ok_or(Error::MissingHostHeader));

    let mut database = database.write().unwrap();
    database.insert(todo.into());

    let todo = database.last().map(|(id, todo)| {
        NetworkTodo::from_todo(todo, host, id)
    });

    Ok(Some(json::encode(&todo).unwrap()))
}

//Clear the database
fn clear(database: &Database, _context: Context) -> Result<Option<String>, Error> {
    database.write().unwrap().clear();
    Ok(Some("".into()))
}

//Send one particular to-do, selected by its id
fn get_todo(database: &Database, context: Context) -> Result<Option<String>, Error> {
    let host = try!(context.headers.get().ok_or(Error::MissingHostHeader));
    let id = try!(context.variables.parse("id").map_err(|_| Error::BadId));

    let todo = database.read().unwrap().get(id).map(|todo| {
        NetworkTodo::from_todo(&todo, host, id)
    });

    Ok(todo.map(|todo| json::encode(&todo).unwrap()))
}

//Update a to-do, selected by its id with data from the request body
fn edit_todo(database: &Database, mut context: Context) -> Result<Option<String>, Error> {
    let edits: NetworkTodo = try!(
        context.body
        .decode_json_body()
        .map_err(|_| Error::ParseError)
    );
    let host = try!(context.headers.get().ok_or(Error::MissingHostHeader));
    let id = try!(context.variables.parse("id").map_err(|_| Error::BadId));

    let mut database =  database.write().unwrap();
    let mut todo = database.get_mut(id);
    todo.as_mut().map(|mut todo| todo.update(edits));

    let todo = todo.map(|todo| {
        NetworkTodo::from_todo(&todo, host, id)
    });

    Ok(Some(json::encode(&todo).unwrap()))
}

//Delete a to-do, selected by its id
fn delete_todo(database: &Database, context: Context) -> Result<Option<String>, Error> {
    let id = try!(context.variables.parse("id").map_err(|_| Error::BadId));
    database.write().unwrap().delete(id);
    Ok(Some("".into()))
}




//An API endpoint with an optional action
struct Api(Option<fn(&Database, Context) -> Result<Option<String>, Error>>);

impl Handler for Api {
    fn handle_request(&self, context: Context, mut response: Response) {
        //Collect the accepted methods from the provided hyperlinks
        let mut methods: Vec<_> = context.hyperlinks.iter().filter_map(|l| l.method.clone()).collect();
        methods.push(context.method.clone());

        //Setup cross origin resource sharing
        response.headers_mut().set(AccessControlAllowOrigin::Any);
        response.headers_mut().set(AccessControlAllowMethods(methods));
        response.headers_mut().set(AccessControlAllowHeaders(vec![UniCase("content-type".into())]));

        //Get the database from the global storage
        let database = if let Some(database) = context.global.get() {
            database
        } else {
            error!("expected a globally accessible Database");
            response.set_status(StatusCode::InternalServerError);
            return
        };

        if let Some(action) = self.0 {
            response.send(action(database, context));
        }
    }
}

//A read-write-locked Table will do as our database
type Database = RwLock<Table>;

//A simple imitation of a database table
struct Table {
    next_id: usize,
    items: BTreeMap<usize, Todo>
}

impl Table {
    fn new() -> Table {
        Table {
            next_id: 0,
            items: BTreeMap::new()
        }
    }

    fn insert(&mut self, item: Todo) {
        self.items.insert(self.next_id, item);
        self.next_id += 1;
    }

    fn delete(&mut self, id: usize) {
        self.items.remove(&id);
    }

    fn clear(&mut self) {
        self.items.clear();
    }

    fn last(&self) -> Option<(usize, &Todo)> {
        self.items.keys().next_back().cloned().and_then(|id| {
            self.items.get(&id).map(|item| (id, item))
        })
    }

    fn get(&self, id: usize) -> Option<&Todo> {
        self.items.get(&id)
    }

    fn get_mut(&mut self, id: usize) -> Option<&mut Todo> {
        self.items.get_mut(&id)
    }

    fn iter(&self) -> Iter<usize, Todo> {
        (&self.items).iter()
    }
}


//A structure for what will be sent and received over the network
#[derive(RustcDecodable, RustcEncodable)]
struct NetworkTodo {
    title: Option<String>,
    completed: Option<bool>,
    order: Option<u32>,
    url: Option<String>
}

impl NetworkTodo {
    fn from_todo(todo: &Todo, host: &Host, id: usize) -> NetworkTodo {
        let url = if let Some(port) = host.port {
            format!("http://{}:{}/{}", host.hostname, port, id)
        } else {
            format!("http://{}/{}", host.hostname, id)
        };

        NetworkTodo {
            title: Some(todo.title.clone()),
            completed: Some(todo.completed),
            order: Some(todo.order),
            url: Some(url)
        }
    }
}


//The stored to-do data
struct Todo {
    title: String,
    completed: bool,
    order: u32
}

impl Todo {
    fn update(&mut self, changes: NetworkTodo) {
        if let Some(title) = changes.title {
            self.title = title;
        }

        if let Some(completed) = changes.completed {
            self.completed = completed;
        }

        if let Some(order) = changes.order {
            self.order = order
        }
    }
}

impl From<NetworkTodo> for Todo {
    fn from(todo: NetworkTodo) -> Todo {
        Todo {
            title: todo.title.unwrap_or(String::new()),
            completed: todo.completed.unwrap_or(false),
            order: todo.order.unwrap_or(0)
        }
    }
}
