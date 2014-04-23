#![crate_id = "rustful#0.1-pre"]

#![comment = "RESTful web framework"]
#![license = "MIT"]
#![crate_type = "lib"]
#![crate_type = "rlib"]

#![doc(html_root_url = "http://www.rust-ci.org/Ogeon/rustful/doc/")]

#![feature(macro_rules)]
#![macro_escape]

#[cfg(test)]
extern crate test;

extern crate collections;
extern crate url;
extern crate time;
extern crate http;

pub use router::Router;
pub use server::Server;
pub use request::Request;
pub use response::Response;

pub mod router;
pub mod server;
pub mod request;
pub mod response;


/**
A macro which creates a router from routes.

It can be used to create a large number of routes without too much path duplication.
Each path can have multiple handlers assigned to it and each handler is connected to
one or multiple HTTP methods. A simple example may look like this:

```
router!(
	"/" => {Get: show_home},
	"/home" => {Get: show_home},
	"/user/:username" => {Get: show_user, Post: save_user},
	"/product/:id" => {Get: show_product, Post | Delete: edit_product}
);
```

This example will result in a router containing these routes:

```
(Get, "/", show_home),
(Get, "/home", show_home),
(Get, "/user/:username", show_user),
(Post, "/user/:username", save_user),
(Get, "/product/:id", show_product),
(Post, "/product/:id", edit_product),
(Delete, "/product/:id", edit_product)
```

The `{}` can be omitted, if there is only one handler for each entry:

```
router!(
	"/" => Get: show_home,
	"/home" => Get: show_home,
	"/user/:username" => Get | Post: handle_user,
	"/product/:id" => Get: show_product,
	"/product/:id" => Post | Delete: edit_product
);
```
**/
#[macro_export]
#[experimental]
macro_rules! router(
	($($path:expr => {$($method:ident: $handler:expr),+}),*) => ({
		let mut router = ::rustful::router::Router::new();

		$(
			$(router.insert_item(::http::method::$method, $path, $handler);)+
		)*

		router
	});

	($($path:expr => {$($($method:ident)|+: $handler:expr),+}),*) => ({
		router!($($path => {$($($method: $handler),+),+}),*)
	});

	($($path:expr => $method:ident: $handler:expr),*) => ({
		let mut router = ::rustful::router::Router::new();

		$(
			router.insert_item(::http::method::$method, $path, $handler);
		)*

		router
	});

	($($path:expr => $($method:ident)|+: $handler:expr),*) => ({
		router!($($path => {$($method: $handler),+}),*)
	});
)


/**
A macro which creates a vector of routes. See `router!(...)` for syntax details.

```
fn main() {
	let routes = routes!("/" => Get: say_hello, "/:person" => Get: say_hello);

	let server = Server {
		handlers: ~Router::from_routes(routes),
		port: 8080
	};

	server.run();
}
```
**/
#[macro_export]
#[experimental]
macro_rules! routes(
	($($path:expr => {$($method:ident: $handler:expr),+}),*) => ({
		[
			$(
				$((::http::method::$method, $path, $handler)),+
			),*
		]
	});

	($($path:expr => {$($($method:ident)|+: $handler:expr),+}),*) => ({
		routes!($($path => {$($($method: $handler),+),+}),*)
	});

	($($path:expr => $method:ident: $handler:expr),*) => ({
		[
			$(
				(::http::method::$method, $path, $handler)
			),*
		]
	});

	($($path:expr => $($method:ident)|+: $handler:expr),*) => ({
		routes!($($path => {$($method: $handler),+}),*)
	});
)


/**
A macro for assigning content types.

It takes either a main type and a sub type or a main type, a sub type and a charset as parameters.
Instead of this:

```
response.headers.content_type = Some(MediaType {
	type_: StrBuf::from_str("text"),
	subtype: StrBuf::from_str("html"),
	parameters: vec!((StrBuf::from_str("charset"), StrBuf::from_str("UTF-8")))
});
```

it can be written like this:

```
response.headers.content_type = content_type!("text", "html");
```

or like this:

```
response.headers.content_type = content_type!("text", "html", "UTF-8");
```
**/
#[macro_export]
#[experimental]
macro_rules! content_type(
	($main_type:expr, $sub_type:expr) => ({
		Some(::http::headers::content_type::MediaType {
			type_: StrBuf::from_str($main_type),
			subtype: StrBuf::from_str($sub_type),
			parameters: vec!((StrBuf::from_str("charset"), StrBuf::from_str("UTF-8")))
		})
	});

	($main_type:expr, $sub_type:expr, $charset:expr) => ({
		Some(::http::headers::content_type::MediaType {
			type_: StrBuf::from_str($main_type),
			subtype: StrBuf::from_str($sub_type),
			parameters: vec!((StrBuf::from_str("charset"), StrBuf::from_str($charset)))
		})
	});
)