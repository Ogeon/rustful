//!Anything related to hypermedia and hyperlinks.

use Method;
use MaybeUtf8Slice;

///Hypermedia connected to an API endpoint.
pub struct Hypermedia<'a> {
    ///Forward links from the current endpoint to other endpoints. These may
    ///include endpoints with the same path, but different method.
    pub links: Vec<Link<'a>>
}

impl<'a> Hypermedia<'a> {
    ///Create an empty `Hypermedia` structure.
    pub fn new() -> Hypermedia<'a> {
        Hypermedia {
            links: vec![]
        }
    }
}

///A hyperlink.
#[derive(PartialEq, Eq, Debug)]
pub struct Link<'a> {
    ///The HTTP method for which an endpoint is available. It can be left
    ///unspecified if the method doesn't matter.
    pub method: Option<Method>,
    ///A relative path from the current location.
    pub path: Vec<LinkSegment<'a>>
}

///A segment of a hyperlink path.
#[derive(PartialEq, Eq, Debug)]
pub enum LinkSegment<'a> {
    ///A static part of a path.
    Static(MaybeUtf8Slice<'a>),
    ///A dynamic part of a path. Can be substituted with anything.
    Variable(MaybeUtf8Slice<'a>),
    ///A recursive wildcard. Will recursively match anything.
    RecursiveWildcard
}