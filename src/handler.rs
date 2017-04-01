//!Request handlers.
use std::borrow::Cow;

use context::Context;
use context::hypermedia::Link;
use response::{Response, SendResponse};
use std::sync::Arc;
use router::RouteState;

///A trait for request handlers.
pub trait Handler: Send + Sync + 'static {
    ///Handle a request from the client. Panicking within this method is
    ///discouraged, to allow the server to run smoothly.
    fn handle(&self, context: Context, response: Response);

    ///Get a description for the handler.
    fn description(&self) -> Option<Cow<'static, str>> {
        None
    }
}

impl<F: Fn(Context, Response) + Send + Sync + 'static> Handler for F {
    fn handle(&self, context: Context, response: Response) {
        self(context, response);
    }
}

impl<T: Handler> Handler for Arc<T> {
    fn handle(&self, context: Context, response: Response) {
        (**self).handle(context, response);
    }
}

impl Handler for Box<Handler> {
    fn handle(&self, context: Context, response: Response) {
        (**self).handle(context, response);
    }
}

///A request environment, containing the context, response and route state.
pub struct Environment<'a, 'b: 'a, 'l, 'g> {
    ///The request context, containing the request parameters.
    pub context: Context<'a, 'b, 'l, 'g>,

    ///The response that will be sent to the client.
    pub response: Response<'a, 'g>,

    ///A representation of the current routing state.
    pub route_state: RouteState<'g>,
}

impl<'a, 'b, 'l, 'g> Environment<'a, 'b, 'l, 'g> {
    ///Replace the hyperlinks. This consumes the whole request environment and
    ///returns a new one with a different lifetime, together with the old
    ///hyperlinks.
    pub fn replace_hyperlinks<'n>(self, hyperlinks: Vec<Link<'n>>) -> (Environment<'a, 'b, 'n, 'g>, Vec<Link<'l>>) {
        let (context, old_hyperlinks) = self.context.replace_hyperlinks(hyperlinks);
        (
            Environment {
                context: context,
                response: self.response,
                route_state: self.route_state
            },
            old_hyperlinks
        )
    }
}

///A low level request handler.
///
///Suitable for implementing routers, since it has complete access to the
///routing environment and can reject the request.
pub trait HandleRequest: Send + Sync + 'static {
    ///Try to handle an incoming request from the client, or return the
    ///request environment to the parent handler.
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>>;

    ///List all of the hyperlinks into this handler, based on the provided
    ///base link. It's up to the handler implementation to decide how deep to
    ///go.
    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>>;
}

impl<H: Handler> HandleRequest for H {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        self.handle(environment.context, environment.response);
        Ok(())
    }

    fn hyperlinks<'a>(&'a self, mut base_link: Link<'a>) -> Vec<Link<'a>> {
        base_link.handler = Some(self);
        vec![base_link]
    }
}

impl<H: HandleRequest> HandleRequest for Option<H> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(ref handler) = *self {
            handler.handle_request(environment)
        } else {
            Err(environment)
        }
    }

    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>> {
        if let Some(ref handler) = *self {
            handler.hyperlinks(base_link)
        } else {
            vec![]
        }
    }
}

///An adapter for simple content creation handlers.
///
///```
///use rustful::{DefaultRouter, Context, Server};
///use rustful::Method::Get;
///use rustful::handler::ContentFactory;
///use rustful::router::Insert;
///
///type Router<T> = DefaultRouter<ContentFactory<T>>;
///
///let mut router = Router::new();
///router.insert(Get, "*", (|context: Context| format!("Visiting {}", context.uri_path)).into());
///
///let server = Server {
///    handlers: router,
///    ..Server::default()
///};
///```
pub struct ContentFactory<T: CreateContent>(T);

impl<T: CreateContent> Handler for ContentFactory<T> {
    fn handle(&self, context: Context, response: Response) {
        response.send(self.0.create_content(context));
    }
}

impl<T: CreateContent> From<T> for ContentFactory<T> {
    fn from(factory: T) -> ContentFactory<T> {
        ContentFactory(factory)
    }
}

///A simplified handler that returns the response content.
///
///It's meant for situations where direct access to the `Response` is
///unnecessary. The benefit is that it can use early returns in a more
///convenient way.
pub trait CreateContent: Send + Sync + 'static {
    ///The type of content that is created.
    type Output: for<'a, 'b> SendResponse<'a, 'b>;

    ///Create content that will be sent to the client.
    fn create_content(&self, context: Context) -> Self::Output;
}

impl<T, R> CreateContent for T where
    T: Fn(Context) -> R + Send + Sync + 'static,
    R: for<'a, 'b> SendResponse<'a, 'b>
{
    type Output = R;

    fn create_content(&self, context: Context) -> Self::Output {
        self(context)
    }
}
