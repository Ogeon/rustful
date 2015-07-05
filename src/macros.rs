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
///use rustful::TreeRouter;
///# use rustful::{Handler, Context, Response};
///
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
///        "/" => Get: show_welcome,
///        "/about" => Get: about_us,
///        "/user/:user" => Get: show_user,
///        "/product/:name" => Get: show_product,
///        "/*" => Get: show_error
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
///use rustful::TreeRouter;
///# use rustful::{Handler, Context, Response};
///
///# #[derive(Clone, Copy)]
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
///        Get: show_home,
///
///        "home" => Get: show_home,
///        "user/:username" => {
///            Get: show_user,
///            Post: save_user
///        },
///        "product" => {
///            Get: show_all_products,
///
///            "json" => Get: send_all_product_data,
///            ":id" => {
///                Get: show_product,
///                Post: edit_product,
///                Delete: edit_product,
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
            __rustful_insert_internal!(router, [], $($paths)+);
            router
        }
    }
}

//Internal stuff. Only meant to be used through `insert_routes!`.
#[doc(hidden)]
#[macro_export]
macro_rules! __rustful_insert_internal {
    ($router:ident, [$($steps:expr),*], $path:expr => {$($paths:tt)+}, $($next:tt)*) => {
        {
            __rustful_insert_internal!($router, [$($steps,)* $path], $($paths)*);
            __rustful_insert_internal!($router, [$($steps),*], $($next)*);
        }
    };
    ($router:ident, [$($steps:expr),*], $path:tt => {$($paths:tt)+}) => {
        {
            __rustful_insert_internal!($router, [$($steps,)* __rustful_to_expr!($path)], $($paths)*);
        }
    };
    ($router:ident, [$($steps:expr),*], $($method:tt)::+: $handler:expr, $($next:tt)*) => {
        {
            let method = {
                #[allow(unused_imports)]
                use $crate::Method::*;
                __rustful_to_path!($($method)::+)
            };
            let path = __rustful_route_expr!($($steps),*);
            $router.insert(method, path, $handler);
            __rustful_insert_internal!($router, [$($steps),*], $($next)*)
        }
    };
    ($router:ident, [$($steps:expr),*], $path:tt => $method:path: $handler:expr, $($next:tt)*) => {
        {
            let method = {
                #[allow(unused_imports)]
                use $crate::Method::*;
                $method
            };
            let path = __rustful_route_expr!($($steps,)* __rustful_to_expr!($path));
            $router.insert(method, path, $handler);
            __rustful_insert_internal!($router, [$($steps),*], $($next)*)
        }
    };
    ($router:ident, [$($steps:expr),*], $($method:tt)::+: $handler:expr) => {
        {
            let method = {
                #[allow(unused_imports)]
                use $crate::Method::*;
                __rustful_to_path!($($method)::+)
            };
            let path = __rustful_route_expr!($($steps),*);
            $router.insert(method, path, $handler);
        }
    };
    ($router:ident, [$($steps:expr),*], $path:tt => $method:path: $handler:expr) => {
        {
            let method = {
                #[allow(unused_imports)]
                use $crate::Method::*;
                $method
            };
            let path = __rustful_route_expr!($($steps,)* __rustful_to_expr!($path));
            $router.insert(method, path, $handler);
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rustful_route_expr {
    () => ("");
    ($($path:expr),+) => (&[$($path),+][..]);
}

/**
A macro for making content types.

It takes a main type, a sub type and a parameter list. Instead of this:

```rust
use rustful::header::ContentType;
use rustful::mime::{Mime, TopLevel, SubLevel, Attr, Value};

ContentType(
    Mime (
        TopLevel::Text,
        SubLevel::Html,
        vec![(Attr::Charset, Value::Utf8)]
    )
);
```

it can be written like this:

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!(Text / Html; Charset = Utf8));
# }
```

The `Charset = Utf8` part defines the parameter list for the content type and
may contain more than one parameter, or be omitted. Here are some more
examples showing that and how strings can be used for more exotic values:

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!(Application / "octet-stream"; "type" = "image/gif"; "padding" = "4"));
# }
```

```
#[macro_use]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!(Image / Png));
# }
```
**/
#[macro_export]
macro_rules! content_type {
    ($main_type:tt / $sub_type:tt) => ({
        use $crate::macros::MimeHelper;
        $crate::mime::Mime (
            {
                #[allow(unused_imports)]
                use $crate::mime::TopLevel::*;
                MimeHelper::from(__rustful_to_expr!($main_type)).convert()
            },
            {
                #[allow(unused_imports)]
                use $crate::mime::SubLevel::*;
                MimeHelper::from(__rustful_to_expr!($sub_type)).convert()
            },
            Vec::new()
        )
    });

    ($main_type:tt / $sub_type:tt; $($param:tt = $value:tt);+) => ({
        use $crate::macros::MimeHelper;
        $crate::mime::Mime (
            {
                #[allow(unused_imports)]
                use $crate::mime::TopLevel::*;
                MimeHelper::from(__rustful_to_expr!($main_type)).convert()
            },
            {
                #[allow(unused_imports)]
                use $crate::mime::SubLevel::*;
                MimeHelper::from(__rustful_to_expr!($sub_type)).convert()
            },
            vec![ $( ({
                #[allow(unused_imports)]
                use $crate::mime::Attr::*;
                MimeHelper::from(__rustful_to_expr!($param)).convert()
            }, {
                #[allow(unused_imports)]
                use $crate::mime::Value::*;
                MimeHelper::from(__rustful_to_expr!($value)).convert()
            })),+ ]
        )
    });
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rustful_to_expr {
    ($e: expr) => ($e)
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rustful_to_path {
    ($e: path) => ($e)
}

use std::str::FromStr;
use std::fmt::Debug;
use mime::{TopLevel, SubLevel, Attr, Value};

#[doc(hidden)]
pub enum MimeHelper<'a, T> {
    Str(&'a str),
    Target(T)
}

impl<'a, T: FromStr> MimeHelper<'a, T> where <T as FromStr>::Err: Debug {
    pub fn convert(self) -> T {
        match self {
            MimeHelper::Str(s) => s.parse().unwrap(),
            MimeHelper::Target(t) => t
        }
    }
}

impl<'a, T: FromStr> From<&'a str> for MimeHelper<'a, T> {
    fn from(s: &'a str) -> MimeHelper<'a, T> {
        MimeHelper::Str(s)
    }
}

impl<'a> From<TopLevel> for MimeHelper<'a, TopLevel> {
    fn from(t: TopLevel) -> MimeHelper<'a, TopLevel> {
        MimeHelper::Target(t)
    }
}

impl<'a> From<SubLevel> for MimeHelper<'a, SubLevel> {
    fn from(t: SubLevel) -> MimeHelper<'a, SubLevel> {
        MimeHelper::Target(t)
    }
}

impl<'a> From<Attr> for MimeHelper<'a, Attr> {
    fn from(t: Attr) -> MimeHelper<'a, Attr> {
        MimeHelper::Target(t)
    }
}

impl<'a> From<Value> for MimeHelper<'a, Value> {
    fn from(t: Value) -> MimeHelper<'a, Value> {
        MimeHelper::Target(t)
    }
}
