//!Routers stores request handlers, using an HTTP method and a path as keys.
//!
//!# Building Routers
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
//!The default tree router is actually a composition of three routers:
//![`TreeRouter`][tree_router], [`MethodRouter`][method_router] and
//![`Variables`][variables]. They come together as the type
//!`TreeRouter<MethodRouter<Variables<_>>>`, but the `TreeRouter` assumes that
//!this is most probably what you want, so this is what `TreeRouter::new()`
//!gives you. No need to write it all out in most of the cases.
//!
//!There may, however, be cases where you want something else. What if you
//!don't care about the HTTP method? Maybe your handler takes care of that
//!internally. Sure, no problem:
//!
//!```
//!use rustful::TreeRouter;
//!use rustful::router::Variables;
//!
//!let my_router = TreeRouter::<Option<Variables<_>>>::default();
//!# let _r: TreeRouter<Option<Variables<DummyHandler>>> = my_router;
//!# struct DummyHandler;
//!# impl rustful::Handler for DummyHandler {
//!#     fn handle_request(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!And what about those route variables? Not using them at all? Well, just
//!remove them too, if you don't want them:
//!
//!```
//!use rustful::TreeRouter;
//!
//!let my_router = TreeRouter::<Option<_>>::default();
//!# let _r: TreeRouter<Option<DummyHandler>> = my_router;
//!# struct DummyHandler;
//!# impl rustful::Handler for DummyHandler {
//!#     fn handle_request(&self, _: rustful::Context, _: rustful::Response){}
//!# }
//!```
//!
//!You can simply recombine and reorder the router types however you want, or
//!why not make your own router? Just implement the `Router` trait.
//!
//![tree_router]: struct.TreeRouter.html
//![method_router]: struct.MethodRouter.html
//![variables]: struct.Variables.html

use std::collections::HashMap;
use std::iter::{Iterator, FlatMap, Peekable};
use std::slice::Split;
use std::ops::Deref;
use std::marker::PhantomData;
use hyper::method::Method;

use handler::Factory;
use context::MaybeUtf8Owned;
use context::hypermedia::Link;

pub use self::tree_router::TreeRouter;
pub use self::method_router::MethodRouter;
pub use self::variables::Variables;

mod tree_router;
mod method_router;
mod variables;

///API endpoint data.
pub struct Endpoint<'a, T: 'a> {
    ///A request handler, if found.
    pub handler: Option<&'a T>,
    ///Path variables for the matching endpoint. May be empty, depending on
    ///the router implementation.
    pub variables: HashMap<MaybeUtf8Owned, MaybeUtf8Owned>,
    ///Any associated hyperlinks.
    pub hyperlinks: Vec<Link<'a>>
}

impl<'a, T> From<Option<&'a T>> for Endpoint<'a, T> {
    fn from(handler: Option<&'a T>) -> Endpoint<'a, T> {
        Endpoint {
            handler: handler,
            variables: HashMap::new(),
            hyperlinks: vec![]
        }
    }
}

///A common trait for routers.
///
///A router must to implement this trait to be usable in a Rustful server. This
///trait will also make the router compatible with the `insert_routes!` macro.
pub trait Router: Send + Sync {
    ///The request handler type that is stored within this router.
    type Handler: Factory;

    ///Build a new router from a route. The router may choose to ignore
    ///both `method` and `route`, depending on its implementation.
    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: Self::Handler) -> Self;

    ///Insert a new route into the router. The router may choose to ignore
    ///both `method` and `route`, depending on its implementation.
    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: Self::Handler);

    ///Insert an other router at a path. The content of the other router will
    ///be merged with this one and conflicting content will be overwritten.
    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: Self);

    ///Change the router as if it was placed under the provided route.
    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R);

    ///Merge this router with an other one, overwriting conflicting parts.
    fn merge(&mut self, other: Self) where Self: Sized {
        self.insert_router("", other);
    }

    ///Find and return the matching handler and variable values.
    fn find<'a>(&'a self, method: &Method, route: &mut RouteState) -> Endpoint<'a, Self::Handler>;

    ///List all of the hyperlinks into this router, based on the provided base
    ///link. It's up to the router implementation to decide how deep to go.
    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>>;
}

impl<H: Factory> Router for H {
    type Handler = H;

    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(_method: Method, _route: R, item: H) -> H {
        item
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, _method: Method, _route: R, item: H) {
        *self = item;
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, _route: R, router: H) {
        *self = router;
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, _route: R) {}

    fn find<'a>(&'a self, _method: &Method, _route: &mut RouteState) -> Endpoint<'a, H> {
        Some(self).into()
    }

    fn hyperlinks<'a>(&'a self, mut base: Link<'a>) -> Vec<Link<'a>> {
        base.handler = Some(self);
        vec![base]
    }
}

impl<T: Router> Router for Option<T> {
    type Handler = T::Handler;

    fn build<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(method: Method, route: R, item: Self::Handler) -> Option<T> {
        Some(T::build(method, route, item))
    }

    fn insert<'a, R: Into<InsertState<'a, I>>, I: Iterator<Item = &'a [u8]>>(&mut self, method: Method, route: R, item: Self::Handler) {
        match *self {
            Some(ref mut r) => r.insert(method, route, item),
            ref mut s @ None => *s = Some(T::build(method, route, item)),
        }
    }

    fn insert_router<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R, router: Option<T>) {
        if let Some(mut other) = router {
            match *self {
                Some(ref mut r) => r.insert_router(route, other),
                ref mut s @ None => {
                    other.prefix(route);
                    *s = Some(other)
                }
            }
        }
    }

    fn prefix<'a, R: Into<InsertState<'a, I>>, I: Clone + Iterator<Item = &'a [u8]>>(&mut self, route: R) {
        self.as_mut().map(|r| r.prefix(route));
    }

    fn find<'a>(&'a self, method: &Method, route: &mut RouteState) -> Endpoint<'a, Self::Handler> {
        if let Some(ref router) = *self {
            router.find(method, route)
        } else {
            None.into()
        }
    }

    fn hyperlinks<'a>(&'a self, base: Link<'a>) -> Vec<Link<'a>> {
        if let Some(ref router) = *self {
            router.hyperlinks(base)
        } else {
            vec![]
        }
    }
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

const IS_SLASH: &'static fn(&u8) -> bool = & (is_slash as fn(&u8) -> bool);

impl<'a> Route<'a> for str {
    type Segments = RouteIter<Split<'a, u8, &'static fn(&u8) -> bool>>;

    fn segments(&'a self) -> <Self as Route<'a>>::Segments {
        self.as_bytes().segments()
    }
}

impl<'a> Route<'a> for [u8] {
    type Segments = RouteIter<Split<'a, u8, &'static fn(&u8) -> bool>>;

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
            RouteIter::Path(s.split(IS_SLASH))
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
#[derive(Clone)]
pub enum RouteIter<I: Iterator> {
    ///A root path (`/`).
    Root,
    ///A non-root path (`path/to/somewhere`).
    Path(I)
}

impl<I: Iterator> Iterator for RouteIter<I> {
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        match *self {
            RouteIter::Path(ref mut i) => i.next(),
            RouteIter::Root => None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match *self {
            RouteIter::Path(ref i) => i.size_hint(),
            RouteIter::Root => (0, Some(0))
        }
    }
}

///A state object for route insertion.
///
///It works as an iterator over the path segments and will, at the same time,
///record the variable names.
#[derive(Clone)]
pub struct InsertState<'a, I: Iterator<Item=&'a [u8]>> {
    route: Peekable<I>,
    variables: Vec<MaybeUtf8Owned>,
    _p: PhantomData<&'a [u8]>,
}

impl<'a, I: Iterator<Item=&'a [u8]>> InsertState<'a, I> {
    ///Extract the variable names from the parsed path.
    pub fn variables(self) -> Vec<MaybeUtf8Owned> {
        self.variables
    }

    ///Check if there are no more segments.
    pub fn is_empty(&mut self) -> bool {
        self.route.peek().is_none()
    }
}

impl<'a, I: Iterator<Item=&'a [u8]>> Iterator for InsertState<'a, I> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        self.route.next().map(|segment| {
            match segment.get(0) {
                Some(&b'*') | Some(&b':') => self.variables.push(segment[1..].to_owned().into()),
                _ => {}
            }
            segment
        })
    }
}

impl<'a, R: Route<'a> + ?Sized> From<&'a R> for InsertState<'a, R::Segments> {
    fn from(route: &'a R) -> InsertState<'a, R::Segments> {
        InsertState {
            route: route.segments().peekable(),
            variables: vec![],
            _p: PhantomData,
        }
    }
}

///A state object for routing.
pub struct RouteState<'a> {
    route: Vec<&'a [u8]>,
    variables: Vec<Option<usize>>,
    index: usize,
    var_index: usize,
}

impl<'a> RouteState<'a> {
    ///Get the current path segment.
    pub fn get(&self) -> Option<&'a [u8]> {
        self.route.get(self.index).cloned()
    }

    ///Don't include this path segment in a variable.
    pub fn skip(&mut self) {
        self.variables.get_mut(self.index).map(|v| *v = None);
        self.index += 1;
    }

    ///Include this path segment as a variable.
    pub fn keep(&mut self) {
        let v_i = self.var_index;
        self.variables.get_mut(self.index).map(|v| *v = Some(v_i));
        self.index += 1;
        self.var_index += 1;
    }

    ///Extend a previously saved variable value with this path segment, or
    ///save it as a new variable.
    pub fn fuse(&mut self) {
        let v_i = self.var_index;
        self.variables.get_mut(self.index).map(|v| *v = Some(v_i));
        self.index += 1;
    }

    ///Assign names to the saved variables and return them.
    pub fn variables(&self, names: &[MaybeUtf8Owned]) -> HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        let values = self.route.iter().zip(self.variables.iter()).filter_map(|(v, keep)| {
            if let Some(index) = *keep {
                Some((index, *v))
            } else {
                None
            }
        });

        let mut var_map = HashMap::<MaybeUtf8Owned, MaybeUtf8Owned>::with_capacity(names.len());
        for (name, value) in VariableIter::new(names, values) {
            var_map.insert(name, value);
        }

        var_map
    }

    ///Get a snapshot of a part of the current state.
    pub fn snapshot(&self) -> (usize, usize) {
        (self.index, self.var_index)
    }

    ///Go to a previously recorded state.
    pub fn go_to(&mut self, snapshot: (usize, usize)) {
        let (index, var_index) = snapshot;
        self.index = index;
        self.var_index = var_index;
    }

    ///Check if there are no more segments.
    pub fn is_empty(&self) -> bool {
        self.index == self.route.len()
    }
}

impl<'a, R: Route<'a> + ?Sized> From<&'a R> for RouteState<'a> {
    fn from(route: &'a R) -> RouteState<'a> {
        let route: Vec<_> = route.segments().collect();
        RouteState {
            variables: vec![None; route.len()],
            route: route,
            index: 0,
            var_index: 0,
        }
    }
}

struct VariableIter<'a, I> {
    iter: I,
    names: &'a [MaybeUtf8Owned],
    current: Option<(usize, MaybeUtf8Owned, MaybeUtf8Owned)>
}

impl<'a, I: Iterator<Item=(usize, &'a [u8])>> VariableIter<'a, I> {
    fn new(names: &'a [MaybeUtf8Owned], iter: I) -> VariableIter<'a, I> {
        VariableIter {
            iter: iter,
            names: names,
            current: None
        }
    }
}

impl<'a, I: Iterator<Item=(usize, &'a [u8])>> Iterator for VariableIter<'a, I> {
    type Item=(MaybeUtf8Owned, MaybeUtf8Owned);

    fn next(&mut self) -> Option<Self::Item> {
        for (next_index, next_segment) in &mut self.iter {
            //validate next_index and check if the variable has a name
            debug_assert!(next_index < self.names.len(), format!("invalid variable name index! variable_names.len(): {}, index: {}", self.names.len(), next_index));
            let next_name = match self.names.get(next_index) {
                None => continue,
                Some(n) if n.is_empty() => continue,
                Some(n) => n
            };

            if let Some((index, name, mut segment)) = self.current.take() {
                if index == next_index {
                    //this is a part of the current sequence
                    segment.push_char('/');
                    segment.push_bytes(next_segment);
                    self.current = Some((index, name, segment));
                } else {
                    //the current sequence has ended
                    self.current = Some((next_index, (*next_name).clone(), next_segment.to_owned().into()));
                    return Some((name, segment));
                }
            } else {
                //this is the first named variable
                self.current = Some((next_index, (*next_name).clone(), next_segment.to_owned().into()));
            }
        }

        //return the last variable
        self.current.take().map(|(_, name, segment)| (name, segment))
    }
}
