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
            "*" => "MaybeKnown::Known(::mime::TopLevel::Star)".into(),
            "text" => "MaybeKnown::Known(::mime::TopLevel::Text)".into(),
            "image" => "MaybeKnown::Known(::mime::TopLevel::Image)".into(),
            "audio" => "MaybeKnown::Known(::mime::TopLevel::Audio)".into(),
            "video" => "MaybeKnown::Known(::mime::TopLevel::Image)".into(),
            "application" => "MaybeKnown::Known(::mime::TopLevel::Application)".into(),
            "multipart" => "MaybeKnown::Known(::mime::TopLevel::Multipart)".into(),
            "message" => "MaybeKnown::Known(::mime::TopLevel::Message)".into(),
            "model" => "MaybeKnown::Known(::mime::TopLevel::Model)".into(),
            top => format!("MaybeKnown::Unknown(\"{}\")", top).into()
        };

        let sub: Cow<_> = match or_continue!(mime_type.next()) {
            "*" => "MaybeKnown::Known(::mime::SubLevel::Star)".into(),
            "plain" => "MaybeKnown::Known(::mime::SubLevel::Plain)".into(),
            "html" => "MaybeKnown::Known(::mime::SubLevel::Html)".into(),
            "xml" => "MaybeKnown::Known(::mime::SubLevel::Xml)".into(),
            "javascript" => "MaybeKnown::Known(::mime::SubLevel::Javascript)".into(),
            "css" => "MaybeKnown::Known(::mime::SubLevel::Css)".into(),
            "json" => "MaybeKnown::Known(::mime::SubLevel::Json)".into(),
            "www-form-url-encoded" => "MaybeKnown::Known(::mime::SubLevel::WwwFormUrlEncoded)".into(),
            "form-data" => "MaybeKnown::Known(::mime::SubLevel::FormData)".into(),
            "png" => "MaybeKnown::Known(::mime::SubLevel::Png)".into(),
            "gif" => "MaybeKnown::Known(::mime::SubLevel::Gif)".into(),
            "bmp" => "MaybeKnown::Known(::mime::SubLevel::Bmp)".into(),
            "jpeg" => "MaybeKnown::Known(::mime::SubLevel::Jpeg)".into(),
            sub => format!("MaybeKnown::Unknown(\"{}\")", sub).into()
        };

        for ext in or_continue!(parts.next()).split(' ') {
            types.insert(String::from(ext), format!("({}, {})", top, sub));
        }
    }

    write!(&mut output, "static MIME: ::phf::Map<&'static str, (MaybeKnown<TopLevel>, MaybeKnown<SubLevel>)> = ").unwrap();
    let mut mimes = ::phf_codegen::Map::new();
    for (ext, ty) in &types {
        mimes.entry(&ext[..], ty);
    }
    mimes.build(&mut output).unwrap();

    write!(&mut output, ";\n").unwrap();
}