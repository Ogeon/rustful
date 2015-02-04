#![crate_name = "rustful_macros"]

#![crate_type = "dylib"]

#![doc(html_root_url = "http://ogeon.github.io/rustful/doc/")]

#![feature(plugin_registrar, quote, rustc_private, collections, path, core)]

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
//!#![feature(plugin)]
//!#[plugin]
//!#[no_link]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!let router = insert_routes!{
//!    TreeRouter::new(): {
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
//!#![feature(plugin)]
//!#[plugin]
//!#[no_link]
//!extern crate rustful_macros;
//!
//!extern crate rustful;
//!
//!...
//!
//!let mut router = TreeRouter::new();
//!insert_routes!{
//!    &mut router: {
//!        "/" => Get: show_home,
//!        "home" => Get: show_home,
//!        "user/:username" => {Get: show_user, Post: save_user},
//!        "product" => {
//!            Get: show_all_products,
//!
//!            "json" => Get: send_all_product_data
//!            ":id" => {
//!                Get: show_product,
//!                Post | Delete: edit_product,
//!
//!                "json" => Get: send_product_data
//!            }
//!        }
//!    }
//!};
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
    let expander = Box::new(expand_routes) as Box<TTMacroExpander>;
    reg.register_syntax_extension(token::intern("insert_routes"), NormalTT(expander, None));
}

fn expand_routes<'cx>(cx: &'cx mut ExtCtxt, sp: codemap::Span, tts: &[ast::TokenTree]) -> Box<MacResult + 'cx> {
    let insert_method = cx.ident_of("insert");
    let router_ident = cx.ident_of("router");
    let router_var = cx.expr_ident(sp, router_ident);
    let router_trait_path = cx.path_global(sp, vec![cx.ident_of("rustful"), cx.ident_of("Router")]);
    let router_trait_use = cx.item_use_simple(sp, ast::Inherited, router_trait_path);

    let mut parser = parse::new_parser_from_tts(
        cx.parse_sess(), cx.cfg(), tts.to_vec()
    );

    let mut calls = vec![cx.stmt_item(sp, router_trait_use), cx.stmt_let(sp, true, router_ident, parser.parse_expr())];
    parser.expect(&token::Colon);

    for (path, method, handler) in parse_routes(cx, &mut parser) {
        let path_expr = cx.parse_expr(format!("\"{}\"", path));
        let method_expr = cx.expr_path(method);
        calls.push(cx.stmt_expr(cx.expr_method_call(sp, router_var.clone(), insert_method, vec![method_expr, path_expr, handler])));
    }

    let block = cx.expr_block(cx.block_all(sp, calls, Some(router_var)));

    MacExpr::new(block)
}

fn parse_routes(cx: &mut ExtCtxt, parser: &mut Parser) -> Vec<(String, ast::Path, P<ast::Expr>)> {
    parse_subroutes("", cx, parser)
}

fn parse_subroutes(base: &str, cx: &mut ExtCtxt, parser: &mut Parser) -> Vec<(String, ast::Path, P<ast::Expr>)> {
    let mut routes = Vec::new();

    parser.eat(&token::OpenDelim(token::Brace));

    while !parser.eat(&token::Eof) {
        match parser.parse_optional_str() {
            Some((ref s, _, _)) => {
                if !parser.eat(&token::FatArrow) {
                    parser.expect(&token::FatArrow);
                    break;
                }

                let mut new_base = base.to_string();
                match s.container_as_str() {
                    Some(s) => {
                        new_base.push_str(s.trim_matches('/'));
                        new_base.push_str("/");
                    },
                    None => cx.span_err(parser.span, "invalid path")
                }

                if parser.eat(&token::Eof) {
                    cx.span_err(parser.span, "unexpected end of routing tree");
                }

                if parser.eat(&token::OpenDelim(token::Brace)) {
                    let subroutes = parse_subroutes(&*new_base, cx, parser);
                    routes.push_all(&*subroutes);

                    if parser.eat(&token::CloseDelim(token::Brace)) {
                        if !parser.eat(&token::Comma) {
                            break;
                        }
                    } else {
                        parser.expect_one_of(&[token::Comma, token::CloseDelim(token::Brace)], &[]);
                    }
                } else {
                    for (method, handler) in parse_handler(parser) {
                        routes.push((new_base.clone(), method, handler))
                    }

                    if !parser.eat(&token::Comma) {
                        break;
                    }
                }
            },
            None => {
                for (method, handler) in parse_handler(parser) {
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
        methods.push(parser.parse_path(parser::NoTypesAllowed));

        if parser.eat(&token::Colon) {
            break;
        }

        if !parser.eat(&token::BinOp(token::Or)) {
            parser.expect_one_of(&[token::Colon, token::BinOp(token::Or)], &[]);
        }
    }

    let handler = parser.parse_expr();

    methods.into_iter().map(|m| (m, handler.clone())).collect()
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
        ::rustful::mime::Mime (
            std::str::FromStr::from_str($main_type).unwrap(),
            std::str::FromStr::from_str($sub_type).unwrap(),
            Vec::new()
        )
    });

    ($main_type:expr, $sub_type:expr, $(($param:expr, $value:expr)),+) => ({
        ::rustful::mime::Mime (
            std::str::FromStr::from_str($main_type).unwrap(),
            std::str::FromStr::from_str($sub_type).unwrap(),
            vec!( $( (std::str::FromStr::from_str($param).unwrap(), std::str::FromStr::from_str($value).unwrap()) ),+ )
        )
    });
}


/**
A macro for callig `send` in response and aborting the handle function if it fails.

This macro will print an error to `stdout`.
**/
#[macro_export]
macro_rules! try_send {
    ($writer:expr, $content:expr) => (
        match $writer.send($content) {
            Ok(v) => v,
            Err(::rustful::ResponseError::IoError(e)) => {
                println!("IO error: {}", e);
                return;
            },
            Err(::rustful::ResponseError::PluginError(e)) => {
                println!("plugin error: {}", e);
                return;
            }
        }
    );

    ($writer:expr, $content:expr, $what:expr) => (
        match $writer.send($content) {
            Ok(v) => v,
            Err(::rustful::ResponseError::IoError(e)) => {
                println!("IO error while {}: {}", $what, e);
                return;
            },
            Err(::rustful::ResponseError::PluginError(e)) => {
                println!("plugin error while {}: {}", $what, e);
                return;
            }
        }
    )
}