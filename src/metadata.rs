use std::collections::BTreeMap;

use crate::{
    grpc::headers,
    status::{Code, Status},
};

#[derive(Debug, Clone)]
pub struct MetadataMap {
    pub(crate) map: BTreeMap<String, Vec<String>>,
}

impl MetadataMap {
    pub fn new() -> Self {
        MetadataMap {
            map: BTreeMap::new(),
        }
    }

    pub fn merge_http_header(&mut self, headers: &hyper::HeaderMap) {
        for (k, v) in headers {
            // ignore invalid header
            let _ = self.insert(k.as_str(), String::from_utf8_lossy(v.as_bytes()).as_ref());
        }
    }

    pub fn insert(&mut self, name: &str, value: &str) -> Result<&mut Self, Status> {
        if name.starts_with(headers::RESREVER_NAME_PREFIX) {
            return Err(Status::new(Code::InvalidArgument, "invalid header name"));
        }

        for b in name.chars() {
            // Header-Name → 1*( %x30-39 / %x61-7A / "_" / "-" / ".") ; 0-9 a-z _ - .
            if !matches!(b, '0'..='9' | 'a'..='z' | '_' | '-' | '.') {
                return Err(Status::new(Code::InvalidArgument, "invalid header name"));
            }
        }

        for b in value.chars() {
            // ASCII-Value → 1*( %x20-%x7E ) ; space and printable ASCII
            if !matches!(b, ' '..='~') {
                return Err(Status::new(Code::InvalidArgument, "invalid header value"));
            }
        }

        self.map
            .entry(name.to_string())
            .or_default()
            .push(value.to_string());

        Ok(self)
    }

    pub fn get(&self, name: &str) -> Option<&Vec<String>> {
        self.map.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Vec<String>> {
        self.map.get_mut(name)
    }

    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.map.remove(name)
    }

    pub fn iter(&self) -> Iter {
        Iter(self.map.iter())
    }
}

impl std::ops::Index<&str> for MetadataMap {
    type Output = Vec<String>;

    fn index(&self, index: &str) -> &Self::Output {
        match self.map.get(index) {
            Some(s) => s,
            None => {
                panic!("MetadataMap[{}] did not exist", index)
            }
        }
    }
}

impl<'a> IntoIterator for &'a MetadataMap {
    type IntoIter = Iter<'a>;
    type Item = (&'a str, &'a Vec<String>);

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct Iter<'a>(std::collections::btree_map::Iter<'a, String, Vec<String>>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a Vec<String>);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (k.as_str(), v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl Default for MetadataMap {
    fn default() -> Self {
        MetadataMap::new()
    }
}

mod test {
    #[test]
    fn print_ascii_header_name_table() {
        // Header-Name → 1*( %x30-39 / %x61-7A / "_" / "-" / ".") ; 0-9 a-z _ - .

        print!("const ASCII_HEADER_NAME_TABLE: [bool; 256] = [");

        for b in 0u8..=0xFF {
            if matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'_' | b'-' |b'.') {
                print!("true");
            } else {
                print!("false");
            }

            print!(",");
        }

        println!("];")
    }

    #[test]
    fn print_ascii_header_value_table() {
        // ASCII-Value → 1*( %x20-%x7E ) ; space and printable ASCII

        print!("const ASCII_HEADER_VALUE_TABLE: [bool; 256] = [");

        for b in 0u8..=0xFF {
            if matches!(b, 0x20..=0x7E) {
                print!("true");
            } else {
                print!("false");
            }

            print!(",");
        }

        println!("];")
    }
}
