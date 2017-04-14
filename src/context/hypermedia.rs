//!Anything related to hypermedia and hyperlinks.
use std::fmt;
use std::cmp;

use Method;
use Handler;
use context::MaybeUtf8Slice;

///A hyperlink.
#[derive(Clone,)]
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

impl<'a> cmp::PartialEq for Link<'a> {
    fn eq(&self, other: &Link<'a>) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl<'a> cmp::Eq for Link<'a> {}

impl<'a> cmp::PartialOrd for Link<'a> {
    fn partial_cmp(&self, other: &Link<'a>) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> cmp::Ord for Link<'a> {
    fn cmp(&self, other: &Link<'a>) -> cmp::Ordering {
        let method_ord = match (&self.method, &other.method) {
            (&None, &None) => cmp::Ordering::Equal,
            (&Some(_), &None) => cmp::Ordering::Greater,
            (&None, &Some(_)) => cmp::Ordering::Less,
            (&Some(ref this_method), &Some(ref other_method)) => this_method.as_ref().cmp(&other_method.as_ref()),
        };

        if method_ord == cmp::Ordering::Equal {
            self.path.cmp(&other.path)
        } else {
            method_ord
        }
    }
}

///A segment of a hyperlink path.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LinkSegment<'a> {
    ///The expected static segment or the name of a variable segment. Variable
    ///segments are allowed to have an empty string as label if it's unknown or
    ///unimportant.
    pub label: MaybeUtf8Slice<'a>,
    ///The type of the segment (e.g. static or variable).
    pub ty: SegmentType
}

///The type of a hyperlink segment.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SegmentType {
    ///A static part of a path.
    Static,
    ///A single dynamic part of a path. This will match one arbitrary path segment.
    VariableSegment,
    ///A dynamic sequence of segments. This works like a variable segment, but
    ///will match one or more segments until the rest of the pattern matches.
    VariableSequence,
}