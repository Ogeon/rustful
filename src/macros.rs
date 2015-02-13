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
