#![crate_name = "rustful_macros"]

#![comment = "Macros for rustful"]
#![license = "MIT"]
#![crate_type = "dylib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(macro_rules, plugin_registrar, quote, phase)]

//!This crate provides some helpful macros for rustful, including `router!` and `routes!`.
//!
//!#`router!`
//!The `router!` macro generates a `Router` containing the provided handlers and routing tree.
//!This can be useful to lower the risk of typing errors, among other things.
//!
//!##Example 1
//!
//!```rust ignore
//!#![feature(phase)]
//!#[phase(plugin)]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!
//!let router = router!{
//!	"/about" => Get: about_us,
//!	"/user/:user" => Get: show_user,
//!	"/product/:name" => Get: show_product,
//!	"/*" => Get: show_error,
//!	"/" => Get: show_welcome
//!};
//!```
//!
//!##Example 2
//!
//!```rust ignore
//!#![feature(phase)]
//!#[phase(plugin)]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!
//!let router = router!{
//!	"/" => Get: show_home,
//!	"home" => Get: show_home,
//!	"user/:username" => {Get: show_user, Post: save_user},
//!	"product" => {
//!		Get: show_all_products,
//!
//!		"json" => Get: send_all_product_data
//!		":id" => {
//!			Get: show_product,
//!			Post | Delete: edit_product,
//!
//!			"json" => Get: send_product_data
//!		}
//!	}
//!};
//!```
//!
//!#`routes!`
//!
//!The `routes!` macro generates a vector of routes, which can be used to create a `Router`.
//!It has the same syntax as `routes!`.
//!
//!```rust ignore
//!#![feature(phase)]
//!#[phase(plugin)]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!
//!let routes = routes!{
//!	"/" => Get: show_home,
//!	"home" => Get: show_home,
//!	"user/:username" => {Get: show_user, Post: save_user},
//!	"product" => {
//!		Get: show_all_products,
//!
//!		"json" => Get: send_all_product_data
//!		":id" => {
//!			Get: show_product,
//!			Post | Delete: edit_product,
//!
//!			"json" => Get: send_product_data
//!		}
//!	}
//!};
//!
//!let router = Router::from_routes(routes);
//!```

extern crate syntax;
extern crate rustc;

use std::path::BytesContainer;

use syntax::{ast, codemap};
use syntax::ext::base::{
	ExtCtxt, MacResult, MacExpr,
	NormalTT, TTMacroExpander
};
use syntax::ext::build::AstBuilder;
use syntax::ext::quote::rt::ExtParseUtils;
use syntax::parse;
use syntax::parse::token;
use syntax::parse::parser;
use syntax::parse::parser::Parser;
use syntax::ptr::P;
//use syntax::print::pprust;

use rustc::plugin::Registry;

#[plugin_registrar]
#[doc(hidden)]
pub fn macro_registrar(reg: &mut Registry) {
	let expander = box expand_router as Box<TTMacroExpander>;
	reg.register_syntax_extension(token::intern("router"), NormalTT(expander, None));

	let expander = box expand_routes as Box<TTMacroExpander>;
	reg.register_syntax_extension(token::intern("routes"), NormalTT(expander, None));
}

fn expand_router<'cx>(cx: &'cx mut ExtCtxt, sp: codemap::Span, tts: &[ast::TokenTree]) -> Box<MacResult + 'cx> {
	let router_ident = cx.ident_of("router");
	let insert_method = cx.ident_of("insert_item");

	let mut calls: Vec<P<ast::Stmt>> = vec!(
		cx.stmt_let(sp, true, router_ident, quote_expr!(&cx, ::rustful::Router::new()))
	);

	for (path, method, handler) in parse_routes(cx, tts).into_iter() {
		let path_expr = cx.parse_expr(format!("\"{}\"", path));
		let method_expr = cx.expr_path(method);
		calls.push(cx.stmt_expr(
			cx.expr_method_call(sp, cx.expr_ident(sp, router_ident), insert_method, vec!(method_expr, path_expr, handler))
		));
	}

	let block = cx.expr_block(cx.block(sp, calls, Some(cx.expr_ident(sp, router_ident))));
	
	MacExpr::new(block)
}

fn expand_routes<'cx>(cx: &'cx mut ExtCtxt, sp: codemap::Span, tts: &[ast::TokenTree]) -> Box<MacResult + 'cx> {
	let routes = parse_routes(cx, tts).into_iter().map(|(path, method, handler)| {
		let path_expr = cx.parse_expr(format!("\"{}\"", path));
		let method_expr = cx.expr_path(method);
		mk_tup(sp, vec!(method_expr, path_expr, handler))
	}).collect();

	MacExpr::new(cx.expr_vec(sp, routes))
}

fn parse_routes(cx: &mut ExtCtxt, tts: &[ast::TokenTree]) -> Vec<(String, ast::Path, P<ast::Expr>)> {

	let mut parser = parse::new_parser_from_tts(
		cx.parse_sess(), cx.cfg(), tts.to_vec()
	);

	parse_subroutes("", cx, &mut parser)
}

fn parse_subroutes(base: &str, cx: &mut ExtCtxt, parser: &mut Parser) -> Vec<(String, ast::Path, P<ast::Expr>)> {
	let mut routes = Vec::new();

	while !parser.eat(&token::Eof) {
		match parser.parse_optional_str() {
			Some((ref s, _)) => {
				if !parser.eat(&token::FatArrow) {
					parser.expect(&token::FatArrow);
					break;
				}

				let mut new_base = base.to_string();
				match s.container_as_str() {
					Some(s) => {
						new_base.push_str(s.trim_chars('/'));
						new_base.push_str("/");
					},
					None => cx.span_err(parser.span, "invalid path")
				}

				if parser.eat(&token::Eof) {
					cx.span_err(parser.span, "unexpected end of routing tree");
				}

				if parser.eat(&token::OpenDelim(token::Brace)) {
					let subroutes = parse_subroutes(new_base.as_slice(), cx, parser);
					routes.push_all(subroutes.as_slice());

					if parser.eat(&token::CloseDelim(token::Brace)) {
						if !parser.eat(&token::Comma) {
							break;
						}
					} else {
						parser.expect_one_of([token::Comma, token::CloseDelim(token::Brace)], []);
					}
				} else {
					for (method, handler) in parse_handler(parser).into_iter() {
						routes.push((new_base.clone(), method, handler))
					}

					if !parser.eat(&token::Comma) {
						break;
					}
				}
			},
			None => {
				for (method, handler) in parse_handler(parser).into_iter() {
					routes.push((base.to_string(), method, handler))
				}

				if !parser.eat(&token::Comma) {
					break;
				}
			}
		}
	}

	routes
}

fn parse_handler(parser: &mut Parser) -> Vec<(ast::Path, P<ast::Expr>)> {
	let mut methods = Vec::new();

	loop {
		methods.push(parser.parse_path(parser::NoTypesAllowed).path);

		if parser.eat(&token::Colon) {
			break;
		}

		if !parser.eat(&token::BinOp(token::Or)) {
			parser.expect_one_of([token::Colon, token::BinOp(token::Or)], []);
		}
	}

	let handler = parser.parse_expr();

	methods.into_iter().map(|m| (m, handler.clone())).collect()
}

fn mk_tup(sp: codemap::Span, content: Vec<P<ast::Expr>>) -> P<ast::Expr> {
	dummy_expr(sp, ast::ExprTup(content))
}

fn dummy_expr(sp: codemap::Span, e: ast::Expr_) -> P<ast::Expr> {
	P(ast::Expr {
		id: ast::DUMMY_NODE_ID,
		node: e,
		span: sp,
	})
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
macro_rules! content_type(
	($main_type:expr, $sub_type:expr) => ({
		Some(::http::headers::content_type::MediaType {
			type_: String::from_str($main_type),
			subtype: String::from_str($sub_type),
			parameters: Vec::new()
		})
	});

	($main_type:expr, $sub_type:expr, $($param:expr: $value:expr),+) => ({
		Some(::http::headers::content_type::MediaType {
			type_: String::from_str($main_type),
			subtype: String::from_str($sub_type),
			parameters: vec!( $( (String::from_str($param), String::from_str($value)) ),+ )
		})
	});
)


/**
A macro for callig `send` in response and aborting the handle function if it fails.

This macro will print an error to `stdout` and set the response status to `500 Internal Server Error` if the send fails.
The response status may or may not be written to the client, depending on if the connection is
broken or not, or if the headers have already been written.
**/
#[macro_export]
macro_rules! try_send(
	($response:ident, $content:expr) => (
		match $response.send($content) {
			::rustful::Success => {},
			::rustful::IoError(e) => {
				println!("IO error: {}", e);
				$response.status = http::status::InternalServerError;
				return;
			},
			::rustful::PluginError(e) => {
				println!("plugin error: {}", e);
				$response.status = http::status::InternalServerError;
				return;
			}
		}
	);

	($response:ident, $content:expr while $what:expr) => (
		match $response.send($content) {
			::rustful::Success => {},
			::rustful::IoError(e) => {
				println!("IO error while {}: {}", $what, e);
				$response.status = http::status::InternalServerError;
				return;
			},
			::rustful::PluginError(e) => {
				println!("plugin error while {}: {}", $what, e);
				$response.status = http::status::InternalServerError;
				return;
			}
		}
	)
)