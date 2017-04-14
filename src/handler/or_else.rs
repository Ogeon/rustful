use handler::routing::{Insert, InsertState};
use context::hypermedia::Link;
use handler::{HandleRequest, Environment};
use Method;

/// A router that selects an item from a secondary handler on error.
///
/// ```
/// use rustful::{OrElse, Context, Response};
///
/// fn error_handler(_: Context, response: Response) {
///     let status = response.status();
///     response.send(format!("Status: {}", status));
/// }
///
/// //Every request will end up at error_handler
/// let handler = OrElse::<Option<fn(Context, Response)>, _>::new(None, error_handler);
/// ```
#[derive(Clone)]
pub struct OrElse<A, B> {
    ///The primary handler that will have the first chance to process the
    ///request.
    pub primary: A,

    ///The secondary handler that will take over if the first handler returns
    ///an error.
    pub secondary: B,
}

impl<A, B> OrElse<A, B> {
    ///Create a new `OrElse` with a primary and secondary handler.
    pub fn new(primary: A, secondary: B) -> OrElse<A, B> {
        OrElse {
            primary: primary,
            secondary: secondary,
        }
    }
}

///Handlers will only be inserted into the primary router.
impl<A: Insert<H>, B: Default, H> Insert<H> for OrElse<A, B> {
    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: H) -> OrElse<A, B> {
        OrElse {
            primary: A::build(method, route, item),
            secondary: B::default()
        }
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: H) {
        self.primary.insert(method, route, item);
    }
}

impl<A: HandleRequest, B: HandleRequest> HandleRequest for OrElse<A, B> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        self.primary.handle_request(environment).or_else(|e| self.secondary.handle_request(e))
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        let mut links = self.primary.hyperlinks(base.clone());
        links.extend(self.secondary.hyperlinks(base.clone()));
        links
    }
}

impl<A: Default, B: Default> Default for OrElse<A, B> {
    fn default() -> OrElse<A, B> {
        OrElse {
            primary: A::default(),
            secondary: B::default(),
        }
    }
}
