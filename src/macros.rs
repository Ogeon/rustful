//!Some helpful macros.

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
#[macro_use(content_type)]
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
#[macro_use(content_type)]
extern crate rustful;
use rustful::header::ContentType;

# fn main() {
ContentType(content_type!(Application / "octet-stream"; "type" = "image/gif"; "padding" = "4"));
# }
```

```
#[macro_use(content_type)]
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
                MimeHelper::from(content_type!(@__rustful_to_expr $main_type)).convert()
            },
            {
                #[allow(unused_imports)]
                use $crate::mime::SubLevel::*;
                MimeHelper::from(content_type!(@__rustful_to_expr $sub_type)).convert()
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
                MimeHelper::from(content_type!(@__rustful_to_expr $main_type)).convert()
            },
            {
                #[allow(unused_imports)]
                use $crate::mime::SubLevel::*;
                MimeHelper::from(content_type!(@__rustful_to_expr $sub_type)).convert()
            },
            vec![ $( ({
                #[allow(unused_imports)]
                use $crate::mime::Attr::*;
                MimeHelper::from(content_type!(@__rustful_to_expr $param)).convert()
            }, {
                #[allow(unused_imports)]
                use $crate::mime::Value::*;
                MimeHelper::from(content_type!(@__rustful_to_expr $value)).convert()
            })),+ ]
        )
    });

    (@__rustful_to_expr $e: expr) => ($e);
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
