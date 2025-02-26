use http::{
    HeaderMap, HeaderValue,
    header::{self, Entry},
};

const CHUNKED: &str = "chunked";
const CHUNKED_HEADER: HeaderValue = HeaderValue::from_static(CHUNKED);

fn make_header_value(s: &[u8]) -> HeaderValue {
    HeaderValue::from_bytes(s).expect("valid header value")
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ContentLen {
    Num(usize),
    None,
    Invalid,
}

pub(crate) fn parse_content_len(headers: &HeaderMap) -> ContentLen {
    match headers.get(header::CONTENT_LENGTH) {
        Some(n) => match n.to_str() {
            Ok(n) => match n.parse() {
                Ok(n) => ContentLen::Num(n),
                Err(_) => ContentLen::Invalid,
            },
            Err(_) => ContentLen::Invalid,
        },
        None => ContentLen::None,
    }
}

pub(crate) fn has_chunked_encoding(headers: &HeaderMap) -> bool {
    match headers.get(header::TRANSFER_ENCODING) {
        Some(value) => value
            .as_bytes()
            .split(|&c| c == b',')
            .any(|part| part.trim_ascii() == b"chunked"),
        None => false,
    }
}

pub(crate) fn remove_chunked_encoding(headers: &mut HeaderMap) {
    let Entry::Occupied(mut en) = headers.entry(header::TRANSFER_ENCODING) else {
        return;
    };

    let value = en.get_mut();
    let parts = || value.as_bytes().split(|&c| c == b',');
    if parts().all(|part| part.trim_ascii() != CHUNKED.as_bytes()) {
        return;
    }

    let mut it = parts().filter(|part| part.trim_ascii() != CHUNKED.as_bytes());
    let Some(first) = it.next() else {
        en.remove();
        return;
    };

    let mut s = Vec::with_capacity(value.len());
    s.extend(first);
    for part in it {
        s.push(b',');
        s.extend(part);
    }

    *value = make_header_value(&s);
}

pub(crate) fn insert_chunked_encoding(headers: &mut HeaderMap) {
    match headers.entry(header::TRANSFER_ENCODING) {
        Entry::Occupied(mut en) => {
            let value = en.get_mut();

            if value
                .as_bytes()
                .split(|&c| c == b',')
                .any(|part| part.trim_ascii() == CHUNKED.as_bytes())
            {
                return;
            }

            *value = if value.as_bytes().trim_ascii().is_empty() {
                CHUNKED_HEADER
            } else {
                let mut s = Vec::with_capacity(value.len());
                s.extend(value.as_bytes());
                s.push(b',');
                s.extend(CHUNKED.as_bytes());
                make_header_value(&s)
            }
        }
        Entry::Vacant(en) => {
            en.insert(CHUNKED_HEADER);
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, http::HeaderName};

    fn headers(name: HeaderName, value: &'static str) -> HeaderMap {
        [(name, HeaderValue::from_static(value))]
            .into_iter()
            .collect()
    }

    #[test]
    fn content_len_num() {
        let headers = headers(header::CONTENT_LENGTH, "10");
        assert_eq!(parse_content_len(&headers), ContentLen::Num(10));
    }

    #[test]
    fn content_len_none() {
        let h = headers(HeaderName::from_static("foo"), "bar");
        assert_eq!(parse_content_len(&h), ContentLen::None);
    }

    #[test]
    fn content_len_invalid() {
        let h = headers(header::CONTENT_LENGTH, "inv");
        assert_eq!(parse_content_len(&h), ContentLen::Invalid);
    }

    #[test]
    fn has_none() {
        let h = headers(HeaderName::from_static("foo"), "bar");
        assert!(!has_chunked_encoding(&h));
    }

    #[test]
    fn has_one() {
        let h = headers(header::TRANSFER_ENCODING, "chunked");
        assert!(has_chunked_encoding(&h));
    }

    #[test]
    fn has_list() {
        let h = headers(header::TRANSFER_ENCODING, "bar, chunked , foo");
        assert!(has_chunked_encoding(&h));
    }

    #[test]
    fn remove_none() {
        let mut h = headers(HeaderName::from_static("foo"), "bar");
        remove_chunked_encoding(&mut h);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn remove_once() {
        let mut h = headers(header::TRANSFER_ENCODING, "chunked");
        remove_chunked_encoding(&mut h);
        assert!(h.is_empty());
    }

    #[test]
    fn remove_twice() {
        let mut h = headers(header::TRANSFER_ENCODING, "chunked,chunked");
        remove_chunked_encoding(&mut h);
        assert!(h.is_empty());
    }

    #[test]
    fn remove_list() {
        let mut h = headers(header::TRANSFER_ENCODING, "chunked,foo");
        remove_chunked_encoding(&mut h);
        assert_eq!(
            h.get(header::TRANSFER_ENCODING),
            Some(&HeaderValue::from_static("foo")),
        );
    }

    #[test]
    fn remove_list_twice() {
        let mut h = headers(header::TRANSFER_ENCODING, "chunked ,foo, chunked");
        remove_chunked_encoding(&mut h);
        assert_eq!(
            h.get(header::TRANSFER_ENCODING),
            Some(&HeaderValue::from_static("foo")),
        );
    }

    #[test]
    fn insert() {
        let mut h = headers(HeaderName::from_static("foo"), "bar");
        insert_chunked_encoding(&mut h);
        assert_eq!(h.len(), 2);
        assert_eq!(
            h.get(header::TRANSFER_ENCODING),
            Some(&HeaderValue::from_static("chunked")),
        );
    }

    #[test]
    fn insert_none() {
        let mut h = headers(header::TRANSFER_ENCODING, "chunked");
        insert_chunked_encoding(&mut h);
        assert_eq!(h.len(), 1);
        assert_eq!(
            h.get(header::TRANSFER_ENCODING),
            Some(&HeaderValue::from_static("chunked")),
        );
    }

    #[test]
    fn insert_list() {
        let mut h = headers(header::TRANSFER_ENCODING, "foo");
        insert_chunked_encoding(&mut h);
        assert_eq!(h.len(), 1);
        assert_eq!(
            h.get(header::TRANSFER_ENCODING),
            Some(&HeaderValue::from_static("foo,chunked")),
        );
    }
}
