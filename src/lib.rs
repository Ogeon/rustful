#![crate_id = "rustful#0.1-pre"]

#![comment = "RESTful web framework"]
#![license = "MIT"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#![doc(html_root_url = "http://www.rust-ci.org/Ogeon/rustful/doc/")]

#![feature(macro_rules, macro_registrar, managed_boxes, quote, phase)]
#![macro_escape]

#[cfg(test)]
extern crate test;

extern crate syntax;

extern crate collections;
extern crate url;
extern crate time;
extern crate http;

pub use router::Router;
pub use server::Server;
pub use request::Request;
pub use response::Response;

use syntax::{ast, codemap};
use syntax::ext::base::{
	SyntaxExtension, ExtCtxt, MacResult, MacExpr,
	NormalTT, BasicMacroExpander
};
use syntax::ext::build::AstBuilder;
use syntax::ext::quote::rt::ExtParseUtils;
use syntax::parse;
use syntax::parse::token;
use syntax::parse::parser;
use syntax::parse::parser::Parser;
//use syntax::print::pprust;

pub mod router;
pub mod server;
pub mod request;
pub mod response;


#[macro_registrar]
#[doc(hidden)]
pub fn macro_registrar(register: |ast::Name, SyntaxExtension|) {
	let expander = ~BasicMacroExpander{expander: expand_router, span: None};
	register(token::intern("router"), NormalTT(expander, None));

	let expander = ~BasicMacroExpander{expander: expand_routes, span: None};
	register(token::intern("routes"), NormalTT(expander, None));
}

fn expand_router(cx: &mut ExtCtxt, sp: codemap::Span, tts: &[ast::TokenTree]) -> ~MacResult {
	let router_ident = cx.ident_of("router");
	let insert_method = cx.ident_of("insert_item");

	let mut calls: Vec<@ast::Stmt> = vec!(
		cx.stmt_let(sp, true, router_ident, quote_expr!(&cx, ::rustful::Router::new()))
	);

	for (path, method, handler) in parse_routes(cx, tts).move_iter() {
		let path_expr = cx.parse_expr("\"" + path + "\"");
		let method_expr = cx.expr_path(method);
		let handler_expr = cx.expr_path(handler);
		calls.push(cx.stmt_expr(
			cx.expr_method_call(sp, cx.expr_ident(sp, router_ident), insert_method, vec!(method_expr, path_expr, handler_expr))
		));
	}

	let block = cx.expr_block(cx.block(sp, calls, Some(cx.expr_ident(sp, router_ident))));
	
	MacExpr::new(block)
}

fn expand_routes(cx: &mut ExtCtxt, sp: codemap::Span, tts: &[ast::TokenTree]) -> ~MacResult {
	let routes = parse_routes(cx, tts).move_iter().map(|(path, method, handler)| {
		let path_expr = cx.parse_expr("\"" + path + "\"");
		let method_expr = cx.expr_path(method);
		let handler_expr = cx.expr_path(handler);
		mk_tup(sp, vec!(method_expr, path_expr, handler_expr))
	}).collect();

	MacExpr::new(cx.expr_vec(sp, routes))
}

fn parse_routes(cx: &mut ExtCtxt, tts: &[ast::TokenTree]) -> Vec<(~str, ast::Path, ast::Path)> {

	let mut parser = parse::new_parser_from_tts(
		cx.parse_sess(), cx.cfg(), Vec::from_slice(tts)
	);

	parse_subroutes("", cx, &mut parser)
}

fn parse_subroutes(base: &str, cx: &mut ExtCtxt, parser: &mut Parser) -> Vec<(~str, ast::Path, ast::Path)> {
	let mut routes = Vec::new();

	while !parser.eat(&token::EOF) {
		match parser.parse_optional_str() {
			Some((ref s, _)) => {
				if !parser.eat(&token::FAT_ARROW) {
					parser.expect(&token::FAT_ARROW);
					break;
				}

				let new_base = base + s.to_str().trim_chars('/').to_owned() + "/";


				if parser.eat(&token::EOF) {
					cx.span_err(parser.span, "unexpected end of routing tree");
				}

				if parser.eat(&token::LBRACE) {
					let subroutes = parse_subroutes(new_base, cx, parser);
					routes.push_all(subroutes.as_slice());

					if parser.eat(&token::RBRACE) {
						if !parser.eat(&token::COMMA) {
							break;
						}
					} else {
						parser.expect_one_of([token::COMMA, token::RBRACE], []);
					}
				} else {
					for (method, handler) in parse_handler(parser).move_iter() {
						routes.push((new_base.clone(), method, handler))
					}

					if !parser.eat(&token::COMMA) {
						break;
					}
				}
			},
			None => {
				for (method, handler) in parse_handler(parser).move_iter() {
					routes.push((base.to_owned(), method, handler))
				}

				if !parser.eat(&token::COMMA) {
					break;
				}
			}
		}
	}

	routes
}

fn parse_handler(parser: &mut Parser) -> Vec<(ast::Path, ast::Path)> {
	let mut methods = Vec::new();

	loop {
		methods.push(parser.parse_path(parser::NoTypesAllowed).path);

		if parser.eat(&token::COLON) {
			break;
		}

		if !parser.eat(&token::BINOP(token::OR)) {
			parser.expect_one_of([token::COLON, token::BINOP(token::OR)], []);
		}
	}

	let handler = parser.parse_path(parser::NoTypesAllowed).path;

	methods.move_iter().map(|m| (m, handler.clone())).collect()
}

fn mk_tup(sp: codemap::Span, content: Vec<@ast::Expr>) -> @ast::Expr {
	dummy_expr(sp, ast::ExprTup(content))
}

fn dummy_expr(sp: codemap::Span, e: ast::Expr_) -> @ast::Expr {
	@ast::Expr {
		id: ast::DUMMY_NODE_ID,
		node: e,
		span: sp,
	}
}


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