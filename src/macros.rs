//!This crate provides some helpful macros for rustful, including `insert_routes!` and `content_type!`.
//!
//!#`insert_routes!`
//!The `insert_routes!` macro generates routes from the provided handlers and routing tree and
//!adds them to the provided router. The router is then returned.
//!
//!This can be useful to lower the risk of typing errors, among other things.
//!
//!##Example 1
//!
//!```rust ignore
//!#[macro_use]
//!extern crate rustful;
//!
//!//...
//!
//!let router = insert_routes! {
//!    TreeRouter::new() => {
//!        "/about" => Get: about_us,
//!        "/user/:user" => Get: show_user,
//!        "/product/:name" => Get: show_product,
//!        "/*" => Get: show_error,
//!        "/" => Get: show_welcome
//!    }
//!};
//!```
//!
//!##Example 2
//!
//!```rust ignore
//!#[macro_use]
//!extern crate rustful;
//!
//!//...
//!
//!let mut router = TreeRouter::new();
//!insert_routes! {
//!    &mut router => {
//!        "/" => Get: show_home,
//!        "home" => Get: show_home,
//!        "user/:username" => {Get: show_user, Post: save_user},
//!        "product" => {
//!            Get: show_all_products,
//!
//!            "json" => Get: send_all_product_data
//!            ":id" => {
//!                "/" => Get: show_product,
//!                "/" => Post: edit_product,
//!                "/" => Delete: edit_product,
//!
//!                "json" => Get: send_product_data
//!            }
//!        }
//!    }
//!};
//!```
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

///Internal stuff. Only meant to be used through `insert_routes!`.
#[macro_export]
macro_rules! __insert_internal {
    ($router:ident, [$($steps:expr),*], $path:expr => {$($paths:tt)+}, $($next:tt)*) => {
        __insert_internal!($router, [$($steps,)* $path], $($paths)*)
        __insert_internal!($router, [$($steps),*], $($next)*)
    };
    ($router:ident, [$($steps:expr),*], $path:expr => {$($paths:tt)+}) => {
        __insert_internal!($router, [$($steps,)* $path], $($paths)*)
    };
    ($router:ident, [$($steps:expr),*], $path:expr => $method:path: $handler:expr, $($next:tt)*) => {
        {
            let path = vec![$($steps,)* $path].connect("/");
            $router.insert($method, &path, $handler);
            __insert_internal!($router, [$($steps),*], $($next)*)
        }
    };
    ($router:ident, [$($steps:expr),*], $path:expr => $method:path: $handler:expr) => {
        {
           let path = vec![$($steps,)* $path].connect("/");
            $router.insert($method, &path, $handler);
        }
    };
}

/**
A macro for assigning content types.

It takes a main type, a sub type and a parameter list. Instead of this:

```
response.headers.content_type = Some(MediaType {
    type_: String::from_str("text"),
    subtype: String::from_str("html"),
    parameters: vec!((String::from_str("charset"), String::from_str("UTF-8")))
});
```

it can be written like this:

```
response.headers.content_type = content_type!("text", "html", "charset": "UTF-8");
```

The `"charset": "UTF-8"` part defines the parameter list for the content type.
It may contain more than one parameter, or be omitted:

```
response.headers.content_type = content_type!("application", "octet-stream", "type": "image/gif", "padding": "4");
```

```
response.headers.content_type = content_type!("image", "png");
```
**/
#[macro_export]
macro_rules! content_type {
    ($main_type:expr, $sub_type:expr) => ({
        $crate::mime::Mime (
            std::str::FromStr::from_str($main_type).unwrap(),
            std::str::FromStr::from_str($sub_type).unwrap(),
            Vec::new()
        )
    });

    ($main_type:expr, $sub_type:expr, $(($param:expr, $value:expr)),+) => ({
        $crate::mime::Mime (
            std::str::FromStr::from_str($main_type).unwrap(),
            std::str::FromStr::from_str($sub_type).unwrap(),
            vec!( $( (std::str::FromStr::from_str($param).unwrap(), std::str::FromStr::from_str($value).unwrap()) ),+ )
        )
    });
}
