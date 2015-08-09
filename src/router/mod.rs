//!Routers stores request handlers, using an HTTP method and a path as keys.
//!
//!Rustful provides a tree structured all-round router called `TreeRouter`,
//!but any other type of router can be used, as long as it implements the
//!`Router` trait. This will also make it possible to initialize it using the
//![`insert_routes!`][insert_routes] macro:
//!
//!```
//!#[macro_use]
//!extern crate rustful;
//!use rustful::TreeRouter;
//!# use rustful::{Handler, Context, Response};
//!
//!# struct DummyHandler;
//!# impl Handler for DummyHandler {
//!#     fn handle_request(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = DummyHandler;
//!# let show_user = DummyHandler;
//!# let list_users = DummyHandler;
//!# let show_product = DummyHandler;
//!# let list_products = DummyHandler;
//!# let show_error = DummyHandler;
//!# let show_welcome = DummyHandler;
//!let router = insert_routes! {
//!    TreeRouter::new() => {
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
//!use rustful::{Router, TreeRouter};
//!# use rustful::{Handler, Context, Response};
//!
//!# struct DummyHandler;
//!# impl Handler for DummyHandler {
//!#     fn handle_request(&self, _: Context, _: Response){}
//!# }
//!# fn main() {
//!# let about_us = DummyHandler;
//!# let show_user = DummyHandler;
//!# let list_users = DummyHandler;
//!# let show_product = DummyHandler;
//!# let list_products = DummyHandler;
//!# let show_error = DummyHandler;
//!# let show_welcome = DummyHandler;
//!let mut router = TreeRouter::new();
//!
//!router.insert(Get, &"/", show_welcome);
//!router.insert(Get, &"/about", about_us);
//!router.insert(Get, &"/users", list_users);
//!router.insert(Get, &"/users/:id", show_user);
//!router.insert(Get, &"/products", list_products);
//!router.insert(Get, &"/products/:id", show_product);
//!router.insert(Get, &"/*", show_error);
//!# }
//!```
//!
//![insert_routes]: ../macro.insert_routes!.html

use std::collections::HashMap;
use std::iter::{Iterator, FlatMap};
use std::slice::Split;
use std::ops::Deref;
use hyper::method::Method;

use handler::Handler;
use context::Hypermedia;
use MaybeUtf8Owned;

pub use self::tree_router::TreeRouter;

mod tree_router;

///API endpoint data.
pub struct Endpoint<'a, T: 'a> {
    ///A request handler, if found.
    pub handler: Option<&'a T>,
    ///Path variables for the matching endpoint. May be empty, depending on
    ///the router implementation.
    pub variables: HashMap<MaybeUtf8Owned, MaybeUtf8Owned>,
    ///Any associated hypermedia, such as links.
    pub hypermedia: Hypermedia<'a>
}

impl<'a, T> From<Option<&'a T>> for Endpoint<'a, T> {
    fn from(handler: Option<&'a T>) -> Endpoint<'a, T> {
        Endpoint {
            handler: handler,
            variables: HashMap::new(),
            hypermedia: Hypermedia::new()
        }
    }
}

///A common trait for routers.
///
///A router must to implement this trait to be usable in a Rustful server. This
///trait will also make the router compatible with the `insert_routes!` macro.
pub trait Router: Send + Sync + 'static {
    ///The request handler type that is stored within this router.
    type Handler: Handler;

    ///Insert a new handler into the router.
    fn insert<'a, D: ?Sized + Deref<Target=R> + 'a, R: ?Sized + Route<'a> + 'a>(&mut self, method: Method, route: &'a D, handler: Self::Handler);

    ///Find and return the matching handler and variable values.
    fn find<'a>(&'a self, method: &Method, route: &[u8]) -> Endpoint<'a, Self::Handler>;
}

impl<H: Handler> Router for H {
    type Handler = H;

    fn find<'a>(&'a self, _method: &Method, _route: &[u8]) -> Endpoint<'a, H> {
        Some(self).into()
    }

    fn insert<'a, D: ?Sized + Deref<Target=R> + 'a, R: ?Sized + Route<'a> + 'a>(&mut self, _method: Method, _route: &'a D, _handler: H) {}
}

///A segmented route.
pub trait Route<'a> {
    ///An iterator over route segments.
    type Segments: Iterator<Item=&'a [u8]>;

    ///Create a route segment iterator. The iterator is expected to return
    ///None for a root path (`/`).
    ///
    ///```rust
    ///# use rustful::router::Route;
    ///let root = "/";
    ///assert_eq!(root.segments().next(), None);
    ///
    ///let path = ["/path", "to/somewhere/", "/", "/else/"];
    ///let segments = path.segments().collect::<Vec<_>>();
    ///let expected = vec![
    ///    "path".as_bytes(),
    ///    "to".as_bytes(),
    ///    "somewhere".as_bytes(),
    ///    "else".as_bytes()
    ///];
    ///assert_eq!(segments, expected);
    ///```
    fn segments(&'a self) -> <Self as Route<'a>>::Segments;
}

fn is_slash(c: &u8) -> bool {
    *c == b'/'
}

impl<'a> Route<'a> for str {
    type Segments = RouteIter<Split<'a, u8, fn(&u8) -> bool>>;

    fn segments(&'a self) -> <Self as Route<'a>>::Segments {
        self.as_bytes().segments()
    }
}

impl<'a> Route<'a> for [u8] {
    type Segments = RouteIter<Split<'a, u8, fn(&u8) -> bool>>;

    fn segments(&'a self) -> <Self as Route<'a>>::Segments {
        let s = if self.starts_with(b"/") {
            &self[1..]
        } else {
            self
        };
        let s = if s.ends_with(b"/") {
            &s[..s.len() - 1]
        } else {
            s
        };

        if s.len() == 0 {
            RouteIter::Root
        } else {
            RouteIter::Path(s.split(is_slash))
        }
    }
}


impl<'a, 'b: 'a, I: 'a, T: 'a> Route<'a> for I where
    &'a I: IntoIterator<Item=&'a T>,
    T: Deref,
    <T as Deref>::Target: Route<'a> + 'b
{
    type Segments = FlatMap<<&'a I as IntoIterator>::IntoIter, <<T as Deref>::Target as Route<'a>>::Segments, fn(&'a T) -> <<T as Deref>::Target as Route<'a>>::Segments>;

    fn segments(&'a self) -> Self::Segments {
        fn segments<'a, 'b: 'a, T: Deref<Target=R> + 'b, R: ?Sized + Route<'a, Segments=S> + 'b, S: Iterator<Item=&'a[u8]>>(s: &'a T) -> S {
            s.segments()
        }

        self.into_iter().flat_map(segments)
    }
}

///Utility iterator for when a root path may be hard to represent.
pub enum RouteIter<I: Iterator> {
    ///A root path (`/`).
    Root,
    ///A non-root path (`path/to/somewhere`).
    Path(I)
}

impl<I: Iterator> Iterator for RouteIter<I> {
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        match self {
            &mut RouteIter::Path(ref mut i) => i.next(),
            &mut RouteIter::Root => None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            &RouteIter::Path(ref i) => i.size_hint(),
            &RouteIter::Root => (0, Some(0))
        }
    }
}
