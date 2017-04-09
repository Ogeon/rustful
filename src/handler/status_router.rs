use std::collections::hash_map::HashMap;

use context::hypermedia::Link;
use StatusCode;
use handler::{HandleRequest, Environment};

/// A router that selects an item from an HTTP status code.
///
/// It does neither alter nor preserve the status code of the response.
///
/// ```
/// use rustful::{OrElse, Context, Response, StatusCode, StatusRouter};
///
/// fn on_404(context: Context, response: Response) {
///     response.send(format!("{} was not found.", context.uri_path));
/// }
///
/// fn on_500(_: Context, response: Response) {
///     response.send("Looks like you found a bug");
/// }
///
/// fn on_unexpected(_: Context, response: Response) {
///     let status = response.status();
///     response.send(format!("Unexpected status: {}", status));
/// }
///
/// //Use error_handler.primary to catch specific errors and error_handler.secondary to
/// //catch everything else
/// let mut error_handler = OrElse::new(StatusRouter::new(), on_unexpected);
/// error_handler.primary.on_status(StatusCode::NotFound, on_404 as fn(Context, Response));
/// error_handler.primary.on_status(StatusCode::InternalServerError, on_500);
///
/// //Every request will end up at error_handler
/// let handler = OrElse::<Option<fn(Context, Response)>, _>::new(None, error_handler);
/// ```
#[derive(Clone)]
pub struct StatusRouter<T> {
    items: HashMap<StatusCode, T>
}

impl<T> StatusRouter<T> {
    ///Create an empty `StatusRouter`.
    pub fn new() -> StatusRouter<T> {
        StatusRouter::default()
    }

    ///Insert a handler that will listen for a specific status code.
    pub fn on_status(&mut self, status: StatusCode, handler: T) {
        self.items.insert(status, handler);
    }
}

impl<T: HandleRequest> HandleRequest for StatusRouter<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(item) = self.items.get(&environment.response.status()) {
            item.handle_request(environment)
        } else {
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        self.items.iter().flat_map(|(_, item)| {
            item.hyperlinks(base.clone())
        }).collect()
    }
}

impl<T> Default for StatusRouter<T> {
    fn default() -> StatusRouter<T> {
        StatusRouter {
            items: HashMap::new(),
        }
    }
}
