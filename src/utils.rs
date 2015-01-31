use std::collections::HashMap;
use std::borrow::ToOwned;
use url::percent_encoding::lossy_utf8_percent_decode;

pub fn parse_parameters(source: &[u8]) -> HashMap<String, String> {
    let mut parameters = HashMap::new();
    let source: Vec<u8> = source.iter()
                                .map(|&e| if e == '+' as u8 { ' ' as u8 } else { e })
                                .collect();

    for parameter in source[].split(|&e| e == '&' as u8) {
        let mut parts = parameter.split(|&e| e == '=' as u8);

        match (parts.next(), parts.next()) {
            (Some(name), Some(value)) => {
                let name = lossy_utf8_percent_decode(name);
                let value = lossy_utf8_percent_decode(value);
                parameters.insert(name, value);
            },
            (Some(name), None) => {
                let name = lossy_utf8_percent_decode(name);
                parameters.insert(name, "".to_owned());
            },
            _ => {}
        }
    }

    parameters
}



#[test]
fn parsing_parameters() {
    let parameters = parse_parameters(b"a=1&aa=2&ab=202");
    let a = "1".to_owned();
    let aa = "2".to_owned();
    let ab = "202".to_owned();
    assert_eq!(parameters.get("a"), Some(&a));
    assert_eq!(parameters.get("aa"), Some(&aa));
    assert_eq!(parameters.get("ab"), Some(&ab));
}

#[test]
fn parsing_parameters_with_plus() {
    let parameters = parse_parameters(b"a=1&aa=2+%2B+extra+meat&ab=202+fifth+avenue");
    let a = "1".to_owned();
    let aa = "2 + extra meat".to_owned();
    let ab = "202 fifth avenue".to_owned();
    assert_eq!(parameters.get("a"), Some(&a));
    assert_eq!(parameters.get("aa"), Some(&aa));
    assert_eq!(parameters.get("ab"), Some(&ab));
}

#[test]
fn parsing_strange_parameters() {
    let parameters = parse_parameters(b"a=1=2&=2&ab=");
    let a = "1".to_owned();
    let aa = "2".to_owned();
    let ab = "".to_owned();
    assert_eq!(parameters.get("a"), Some(&a));
    assert_eq!(parameters.get(""), Some(&aa));
    assert_eq!(parameters.get("ab"), Some(&ab));
}