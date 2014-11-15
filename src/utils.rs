use std::collections::HashMap;
use url::percent_encoding::lossy_utf8_percent_decode;

pub fn parse_parameters(source: &[u8]) -> HashMap<String, String> {
    let mut parameters = HashMap::new();
    let source: Vec<u8> = source.iter()
                                .map(|&e| if e == '+' as u8 { ' ' as u8 } else { e })
                                .collect();

    for parameter in source.as_slice().split(|&e| e == '&' as u8) {
        let mut parts = parameter.split(|&e| e == '=' as u8);

        match (parts.next(), parts.next()) {
            (Some(name), Some(value)) => {
                let name = lossy_utf8_percent_decode(name);
                let value = lossy_utf8_percent_decode(value);
                parameters.insert(name, value);
            },
            (Some(name), None) => {
                let name = lossy_utf8_percent_decode(name);
                parameters.insert(name, "".into_string());
            },
            _ => {}
        }
    }

    parameters
}



#[test]
fn parsing_parameters() {
    let parameters = parse_parameters(b"a=1&aa=2&ab=202");
    let a = "1".into_string();
    let aa = "2".into_string();
    let ab = "202".into_string();
    assert_eq!(parameters.get(&"a".into_string()), Some(&a));
    assert_eq!(parameters.get(&"aa".into_string()), Some(&aa));
    assert_eq!(parameters.get(&"ab".into_string()), Some(&ab));
}

#[test]
fn parsing_parameters_with_plus() {
    let parameters = parse_parameters(b"a=1&aa=2+%2B+extra+meat&ab=202+fifth+avenue");
    let a = "1".into_string();
    let aa = "2 + extra meat".into_string();
    let ab = "202 fifth avenue".into_string();
    assert_eq!(parameters.get(&"a".into_string()), Some(&a));
    assert_eq!(parameters.get(&"aa".into_string()), Some(&aa));
    assert_eq!(parameters.get(&"ab".into_string()), Some(&ab));
}

#[test]
fn parsing_strange_parameters() {
    let parameters = parse_parameters(b"a=1=2&=2&ab=");
    let a = "1".into_string();
    let aa = "2".into_string();
    let ab = "".into_string();
    assert_eq!(parameters.get(&"a".into_string()), Some(&a));
    assert_eq!(parameters.get(&"".into_string()), Some(&aa));
    assert_eq!(parameters.get(&"ab".into_string()), Some(&ab));
}