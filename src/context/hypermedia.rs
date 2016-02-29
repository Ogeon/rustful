//!Anything related to hypermedia and hyperlinks.
use std::fmt;

use Method;
use Handler;
use context::MaybeUtf8Slice;

///A hyperlink.
#[derive(Clone)]
pub struct Link<'a> {
    ///The HTTP method for which an endpoint is available. It can be left
    ///unspecified if the method doesn't matter.
    pub method: Option<Method>,
    ///A relative path from the current location.
    pub path: Vec<LinkSegment<'a>>,
    ///The handler that will answer at the endpoint.
    pub handler: Option<&'a Handler>,
}

impl<'a> fmt::Debug for Link<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "method: {:?}, path: {:?}, handler present: {}", self.method, self.path, self.handler.is_some())
    }
}

///A segment of a hyperlink path.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LinkSegment<'a> {
    ///The expected static segment or the name of a variable segment. Variable
    ///segments are allowed to have an empty string as label if it's unknown or
    ///unimportant.
    pub label: MaybeUtf8Slice<'a>,
    ///The type of the segment (e.g. static or variable).
    pub ty: SegmentType
}

///The type of a hyperlink segment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SegmentType {
    ///A static part of a path.
    Static,
    ///A single dynamic part of a path. This will match one arbitrary path segment.
    VariableSegment,
    ///A dynamic sequence of segments. This works like a variable segment, but
    ///will match one or more segments until the rest of the pattern matches.
    VariableSequence,
}