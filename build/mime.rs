use std::io::{Write, BufRead, BufReader};
use std::fs::File;
use std::collections::HashMap;
use std::borrow::Cow;

macro_rules! or_continue {
    ($e: expr) => (if let Some(v) = $e {
        v
    } else {
        continue;
    })
}

pub fn gen() {
    let input = File::open("build/apache/mime.types").unwrap_or_else(|e| {
        panic!("could not open 'build/apache/mime.types': {}", e);
    });

    let out_path = format!("{}/mime.rs", env!("OUT_DIR"));
    let mut output = File::create(&out_path).unwrap_or_else(|e| {
        panic!("could not create '{}': {}", out_path, e);
    });

    let mut types = HashMap::new();

    for line in BufReader::new(input).lines().filter_map(Result::ok) {
        if let Some('#') = line.chars().next() {
            continue;
        }

        let mut parts = line.split('\t').filter(|v| v.len() > 0);
        let mut mime_type = or_continue!(parts.next()).split('/');

        let top: Cow<_> = match or_continue!(mime_type.next()) {
            "*" => "Top::Known(::mime::TopLevel::Star)".into(),
            "text" => "Top::Known(::mime::TopLevel::Text)".into(),
            "image" => "Top::Known(::mime::TopLevel::Image)".into(),
            "audio" => "Top::Known(::mime::TopLevel::Audio)".into(),
            "video" => "Top::Known(::mime::TopLevel::Image)".into(),
            "application" => "Top::Known(::mime::TopLevel::Application)".into(),
            "multipart" => "Top::Known(::mime::TopLevel::Multipart)".into(),
            "message" => "Top::Known(::mime::TopLevel::Message)".into(),
            "model" => "Top::Known(::mime::TopLevel::Model)".into(),
            top => format!("Top::Unknown(\"{}\")", top).into()
        };

        let sub: Cow<_> = match or_continue!(mime_type.next()) {
            "*" => "Sub::Known(::mime::SubLevel::Star)".into(),
            "plain" => "Sub::Known(::mime::SubLevel::Plain)".into(),
            "html" => "Sub::Known(::mime::SubLevel::Html)".into(),
            "xml" => "Sub::Known(::mime::SubLevel::Xml)".into(),
            "javascript" => "Sub::Known(::mime::SubLevel::Javascript)".into(),
            "css" => "Sub::Known(::mime::SubLevel::Css)".into(),
            "json" => "Sub::Known(::mime::SubLevel::Json)".into(),
            "www-form-url-encoded" => "Sub::Known(::mime::SubLevel::WwwFormUrlEncoded)".into(),
            "form-data" => "Sub::Known(::mime::SubLevel::FormData)".into(),
            "png" => "Sub::Known(::mime::SubLevel::Png)".into(),
            "gif" => "Sub::Known(::mime::SubLevel::Gif)".into(),
            "bmp" => "Sub::Known(::mime::SubLevel::Bmp)".into(),
            "jpeg" => "Sub::Known(::mime::SubLevel::Jpeg)".into(),
            sub => format!("Sub::Unknown(\"{}\")", sub).into()
        };

        for ext in or_continue!(parts.next()).split(' ') {
            types.insert(String::from(ext), format!("({}, {})", top, sub));
        }
    }

    write!(&mut output, "static MIME: ::phf::Map<&'static str, (Top, Sub)> = ").unwrap();
    let mut mimes = ::phf_codegen::Map::new();
    for (ext, ty) in &types {
        mimes.entry(&ext[..], ty);
    }
    mimes.build(&mut output).unwrap();

    write!(&mut output, ";\n").unwrap();
}