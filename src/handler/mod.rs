//!Request handlers.
//!
//!# Building Routers
//!
//!Rustful provides a tree structured all-round router called `DefaultRouter`,
//!but any other type of router can be used, as long as it implements the
//![`HandleRequest`][handle_request] trait. Implementing the
//![`Insert`][insert] trait will also make it possible to initialize it using
//!the [`insert_routes!`][insert_routes] macro:
//!
//!```
//!#[macro_use]
//!extern crate rustful;
//!use rustful::DefaultRouter;
//!# use rustful::{Handler, Context, Response};
//!
//!# struct ExampleHandler;
//!# impl Handler for ExampleHandler {
//!#     fn handle(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = ExampleHandler;
//!# let show_user = ExampleHandler;
//!# let list_users = ExampleHandler;
//!# let show_product = ExampleHandler;
//!# let list_products = ExampleHandler;
//!# let show_error = ExampleHandler;
//!# let show_welcome = ExampleHandler;
//!let router = insert_routes! {
//!    DefaultRouter::<ExampleHandler>::new() => {
//!        Get: show_welcome,
//!        "about" => Get: about_us,
//!        "users" => {
//!            Get: list_users,
//!            ":id" => Get: show_user
//!        },
//!        "products" => {
//!            Get: list_products,
//!            ":id" => Get: show_product
//!        },
//!        "*" => Get: show_error
//!    }
//!};
//!# }
//!```
//!
//!This macro creates the same structure as the example below, but it allows
//!tree structures to be defined without the need to write the same paths
//!multiple times. This can be useful to lower the risk of typing errors,
//!among other things.
//!
//!Routes may also be added using the insert method, like this:
//!
//!```
//!extern crate rustful;
//!use rustful::Method::Get;
//!use rustful::{Insert, DefaultRouter};
//!# use rustful::{Handler, Context, Response};
//!
//!# struct ExampleHandler;
//!# impl Handler for ExampleHandler {
//!#     fn handle(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = ExampleHandler;
//!# let show_user = ExampleHandler;
//!# let list_users = ExampleHandler;
//!# let show_product = ExampleHandler;
//!# let list_products = ExampleHandler;
//!# let show_error = ExampleHandler;
//!# let show_welcome = ExampleHandler;
//!let mut router = DefaultRouter::<ExampleHandler>::new();
//!
//!router.insert(Get, "/", show_welcome);
//!router.insert(Get, "/about", about_us);
//!router.insert(Get, "/users", list_users);
//!router.insert(Get, "/users/:id", show_user);
//!router.insert(Get, "/products", list_products);
//!router.insert(Get, "/products/:id", show_product);
//!router.insert(Get, "/*", show_error);
//!# }
//!```
//!
//![insert_routes]: ../macro.insert_routes!.html
//!
//!#Variables
//!
//!Routes may contain variables, that are useful for capturing parts of the
//!requested path as input to the handler. The syntax for a variable is simply
//!an indicator character (`:` or `*`) followed by a label. Variables without
//!labels are also valid, but their values will be discarded.
//!
//!##Variable Segments (:label)
//!
//!A variable segment will match a single arbitrary segment. They are probably
//!the most commonly used variables and may, for example, be used to select a
//!blog post: `"posts/:year/:month/:day/:title_slug"`.
//!
//!```text
//!pattern = "a/:v/b"
//!"a/c/b" -> v = "c"
//!"a/c/d/b" -> no match
//!"a/b" -> no match
//!"a/c/b/d" -> no match
//!```
//!
//!##Variable Sequences (*label)
//!
//!A variable sequence is similar to a variable segment, but with the
//!difference that it may consume multiple segments until the rest of the path
//!gives a match. An example use case is a route for downloadable files that
//!may be arranged in arbitrary directories: `"downloads/*file_path"`.
//!
//!```text
//!pattern = "a/*v/b"
//!"a/c/b" -> v = "c"
//!"a/c/d/b" -> v = "c/d"
//!"a/b" -> no match
//!"a/c/b/d" -> no match
//!```
//!
//!```text
//!pattern = "a/b/*v"
//!"a/b/c" -> v = "c"
//!"a/b/c/d" -> v = "c/d"
//!"a/b" -> no match
//!```
//!
//!# Router Composition
//!
//!The default router is actually a composition of three routers:
//![`TreeRouter`][tree_router], [`MethodRouter`][method_router] and
//![`Variables`][variables]. They come together as the type
//!`DefaultRouter<T>`, so no need to write it all out in most of the cases.
//!
//!There may, however, be cases where you want something else. What if you
//!don't care about the HTTP method? Maybe your handler takes care of that
//!internally. Sure, no problem:
//!
//!```
//!use rustful::handler::{Variables, TreeRouter};
//!
//!let my_router = TreeRouter::<Option<Variables<ExampleHandler>>>::new();
//!# struct ExampleHandler;
//!# impl rustful::Handler for ExampleHandler {
//!#     fn handle(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!And what about those route variables? Not using them at all? Well, just
//!remove them too, if you don't want them:
//!
//!```
//!use rustful::handler::TreeRouter;
//!
//!let my_router = TreeRouter::<Option<ExampleHandler>>::new();
//!# struct ExampleHandler;
//!# impl rustful::Handler for ExampleHandler {
//!#     fn handle(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!You can simply recombine and reorder the router types however you want, or
//!why not make your own router? Just implement
//![`HandleRequest`][handle_request] and the optional [`Insert`][insert]
//!trait.
//!
//![tree_router]: struct.TreeRouter.html
//![method_router]: struct.MethodRouter.html
//![variables]: struct.Variables.html
//![handle_request]: trait.HandleRequest.html
//![insert]: routing/trait.Insert.html

use std::borrow::Cow;
use std::sync::Arc;

use context::Context;
use context::hypermedia::Link;
use response::{Response, SendResponse};
use self::routing::{Insert, RouteState, InsertState};
use {Method, StatusCode};

pub use self::tree_router::TreeRouter;
pub use self::method_router::MethodRouter;
pub use self::variables::Variables;
pub use self::or_else::OrElse;
pub use self::status_router::StatusRouter;

pub mod routing;

mod tree_router;
mod method_router;
mod variables;
mod or_else;
mod status_router;

///Alias for `TreeRouter<MethodRouter<Variables<T>>>`.
///
///This is probably the most common composition, which will select handlers
///based on path and then HTTP method, and collect any variables on the way.
pub type DefaultRouter<T> = TreeRouter<MethodRouter<Variables<T>>>;

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
    fn handle_request<'a, 'b, 'l, 'g>(&self, mut environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        if let Some(ref handler) = *self {
            handler.handle_request(environment)
        } else {
            environment.response.set_status(StatusCode::NotFound);
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
///use rustful::{DefaultRouter, Context, Server, Insert, ContentFactory};
///use rustful::Method::Get;
///
///type Router<T> = DefaultRouter<ContentFactory<T>>;
///
///let mut router = Router::new();
///router.insert(Get, "*", |context: Context| format!("Visiting {}", context.uri_path));
///
///let server = Server {
///    handlers: router,
///    ..Server::default()
///};
///```
pub struct ContentFactory<T: CreateContent>(pub T);

impl<T: CreateContent> HandleRequest for ContentFactory<T> {
    fn handle_request<'a, 'b, 'l, 'g>(&self, environment: Environment<'a, 'b, 'l, 'g>) -> Result<(), Environment<'a, 'b, 'l, 'g>> {
        environment.response.send(self.0.create_content(environment.context));
        Ok(())
    }

    fn hyperlinks<'a>(&'a self, base_link: Link<'a>) -> Vec<Link<'a>> {
        vec![base_link]
    }
}

impl<H: Into<ContentFactory<T>>, T: CreateContent> Insert<H> for ContentFactory<T> {
    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(_method: Method, _route: R, item: H) -> ContentFactory<T> {
        item.into()
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, _method: Method, _route: R, item: H) {
        *self = item.into();
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
