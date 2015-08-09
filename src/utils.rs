use url::percent_encoding::percent_decode;
use Parameters;

pub fn parse_parameters(source: &[u8]) -> Parameters {
    let mut parameters = Parameters::new();
    let source: Vec<u8> = source.iter()
                                .map(|&e| if e == '+' as u8 { ' ' as u8 } else { e })
                                .collect();

    for parameter in source.split(|&e| e == '&' as u8) {
        let mut parts = parameter.split(|&e| e == '=' as u8);

        match (parts.next(), parts.next()) {
            (Some(name), Some(value)) => {
                let name = percent_decode(name);
                let value = percent_decode(value);
                parameters.insert(name, value);
            },
            (Some(name), None) => {
                let name = percent_decode(name);
                parameters.insert(name, String::new());
            },
            _ => {}
        }
    }

    parameters
}

#[cfg(test)]
mod test {
    use std::borrow::ToOwned;
    use super::parse_parameters;

    #[test]
    fn parsing_parameters() {
        let parameters = parse_parameters(b"a=1&aa=2&ab=202");
        let a = "1".to_owned().into();
        let aa = "2".to_owned().into();
        let ab = "202".to_owned().into();
        assert_eq!(parameters.get_raw("a"), Some(&a));
        assert_eq!(parameters.get_raw("aa"), Some(&aa));
        assert_eq!(parameters.get_raw("ab"), Some(&ab));
    }

    #[test]
    fn parsing_parameters_with_plus() {
        let parameters = parse_parameters(b"a=1&aa=2+%2B+extra+meat&ab=202+fifth+avenue");
        let a = "1".to_owned().into();
        let aa = "2 + extra meat".to_owned().into();
        let ab = "202 fifth avenue".to_owned().into();
        assert_eq!(parameters.get_raw("a"), Some(&a));
        assert_eq!(parameters.get_raw("aa"), Some(&aa));
        assert_eq!(parameters.get_raw("ab"), Some(&ab));
    }

    #[test]
    fn parsing_strange_parameters() {
        let parameters = parse_parameters(b"a=1=2&=2&ab=");
        let a = "1".to_owned().into();
        let aa = "2".to_owned().into();
        let ab = "".to_owned().into();
        assert_eq!(parameters.get_raw("a"), Some(&a));
        assert_eq!(parameters.get_raw(""), Some(&aa));
        assert_eq!(parameters.get_raw("ab"), Some(&ab));
    }
}