//!Some helpful macros.

///The `insert_routes!` macro generates routes from the provided handlers and routing tree and
///adds them to the provided router. The router is then returned.
///
///This can be useful to lower the risk of typing errors, among other things.
///
///##Example 1
///
///```rust
///#[macro_use]
///extern crate rustful;
///use rustful::Method::Get;
///# use rustful::{Handler, Context, Response, TreeRouter};
///
///# #[derive(Copy)]
///# struct DummyHandler;
///# impl Handler for DummyHandler {
///#     fn handle_request(&self, _: Context, _: Response){}
///# }
///# fn main() {
///# let about_us = DummyHandler;
///# let show_user = DummyHandler;
///# let show_product = DummyHandler;
///# let show_error = DummyHandler;
///# let show_welcome = DummyHandler;
///let router = insert_routes! {
///    TreeRouter::new() => {
///        "/about" => Get: about_us,
///        "/user/:user" => Get: show_user,
///        "/product/:name" => Get: show_product,
///        "/*" => Get: show_error,
///        "/" => Get: show_welcome
///    }
///};
///# }
///```
///
///##Example 2
///
///```rust
///#[macro_use]
///extern crate rustful;
///use rustful::Method::{Get, Post, Delete};
///# use rustful::{Handler, Context, Response, TreeRouter};
///
///# #[derive(Copy)]
///# struct DummyHandler;
///# impl Handler for DummyHandler {
///#     fn handle_request(&self, _: Context, _: Response){}
///# }
///# fn main() {
///# let show_home = DummyHandler;
///# let show_user = DummyHandler;
///# let save_user = DummyHandler;
///# let show_all_products = DummyHandler;
///# let send_all_product_data = DummyHandler;
///# let show_product = DummyHandler;
///# let edit_product = DummyHandler;
///# let send_product_data = DummyHandler;
///
///let mut router = TreeRouter::new();
///
///insert_routes! {
///    &mut router => {
///        "/" => Get: show_home,
///        "home" => Get: show_home,
///        "user/:username" => {
///            "/" => Get: show_user,
///            "/" => Post: save_user
///        },
///        "product" => {
///            "/" => Get: show_all_products,
///
///            "json" => Get: send_all_product_data,
///            ":id" => {
///                "/" => Get: show_product,
///                "/" => Post: edit_product,
///                "/" => Delete: edit_product,
///
///                "json" => Get: send_product_data
///            }
///        }
///    }
///};
///# }
///```
#[macro_export]
macro_rules! insert_routes {
    ($router:expr => {$($paths:tt)+}) => {
        {
            use $crate::Router;
            let mut router = $router;
            __insert_internal!(router, [], $($paths)+);
            router
        }
    }
}

//Internal stuff. Only meant to be used through `insert_routes!`.
#[doc(hidden)]
#[macro_export]
macro_rules! __insert_internal {
    ($router:ident, [$($steps:expr),*], $path:expr => {$($paths:tt)+}, $($next:tt)*) => {
        {
            __insert_internal!($router, [$($steps,)* $path], $($paths)*);
            __insert_internal!($router, [$($steps),*], $($next)*);
        }
    };
    ($router:ident, [$($steps:expr),*], $path:expr => {$($paths:tt)+}) => {
        {
            __insert_internal!($router, [$($steps,)* $path], $($paths)*);
        }
    };
    ($router:ident, [$($steps:expr),*], $path:expr => $method:path: $handler:expr, $($next:tt)*) => {
        {
            $router.insert($method, &[$($steps,)* $path][..], $handler);
            __insert_internal!($router, [$($steps),*], $($next)*)
        }
    };
    ($router:ident, [$($steps:expr),*], $path:expr => $method:path: $handler:expr) => {
        {
            $router.insert($method, &[$($steps,)* $path][..], $handler);
        }
    };
}

/**
A macro for assigning content types.

It takes a main type, a sub type and a parameter list. Instead of this:

```rust
use rustful::header::ContentType;
use rustful::mime::Mime;

ContentType(
    Mime (
        std::str::FromStr::from_str("text").unwrap(),
        std::str::FromStr::from_str("html").unwrap(),
        vec![(std::str::FromStr::from_str("charset").unwrap(), std::str::FromStr::from_str("UTF-8").unwrap())]
    )
);
```

it can be written like this:

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!("text", "html", ("charset", "UTF-8")));
# }
```

The `"charset": "UTF-8"` part defines the parameter list for the content type.
It may contain more than one parameter, or be omitted:

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!("application", "octet-stream", ("type", "image/gif"), ("padding", "4")));
# }
```

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!("image", "png"));
# }
```
**/
#[macro_export]
macro_rules! content_type {
    ($main_type:expr, $sub_type:expr) => ({
        $crate::mime::Mime (
            ::std::str::FromStr::from_str($main_type).unwrap(),
            ::std::str::FromStr::from_str($sub_type).unwrap(),
            Vec::new()
        )
    });

    ($main_type:expr, $sub_type:expr, $(($param:expr, $value:expr)),+) => ({
        $crate::mime::Mime (
            ::std::str::FromStr::from_str($main_type).unwrap(),
            ::std::str::FromStr::from_str($sub_type).unwrap(),
            vec!( $( (::std::str::FromStr::from_str($param).unwrap(), ::std::str::FromStr::from_str($value).unwrap()) ),+ )
        )
    });
}
