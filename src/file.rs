//!File related utilities.

use mime::{Mime, TopLevel, SubLevel};

include!(concat!(env!("OUT_DIR"), "/mime.rs"));

///Returns the MIME type from a given file extension, if known.
///
///The file extension to MIME type mapping is based on [data from the Apache
///server][apache].
///
///```
///use rustful::file::ext_to_mime;
///use rustful::mime::Mime;
///use rustful::mime::TopLevel::Image;
///use rustful::mime::SubLevel::Jpeg;
///
///let mime = ext_to_mime("jpg");
///assert_eq!(mime, Some(Mime(Image, Jpeg, vec![])));
///```
///
///[apache]: http://svn.apache.org/viewvc/httpd/httpd/trunk/docs/conf/mime.types?view=markup
pub fn ext_to_mime(ext: &str) -> Option<Mime> {
    MIME.get(ext).map(|&(ref top, ref sub)| {
        Mime(top.into(), sub.into(), vec![])
    })
}

enum MaybeKnown<T> {
    Known(T),
    Unknown(&'static str)
}

impl<'a> Into<TopLevel> for &'a MaybeKnown<TopLevel> {
    fn into(self) -> TopLevel {
        match *self {
            MaybeKnown::Known(ref t) => t.clone(),
            MaybeKnown::Unknown(t) => TopLevel::Ext(t.into())
        }
    }
}

impl<'a> Into<SubLevel> for &'a MaybeKnown<SubLevel> {
    fn into(self) -> SubLevel {
        match *self {
            MaybeKnown::Known(ref s) => s.clone(),
            MaybeKnown::Unknown(s) => SubLevel::Ext(s.into())
        }
    }
}
