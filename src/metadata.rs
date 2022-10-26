use std::collections::BTreeMap;

use crate::{
    grpc::headers,
    status::{Code, Status},
};

const ASCII_HEADER_NAME_TABLE: [bool; 256] = [
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, true, true, false, true, true, true, true, true,
    true, true, true, true, true, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, true, false, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false,
];

const ASCII_HEADER_VALUE_TABLE: [bool; 256] = [
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false, false, false, false, false, false,
    false, false, false, false,
];

pub struct Metadata {
    pub(crate) map: BTreeMap<String, Vec<String>>,
}

impl Metadata {
    pub fn new() -> Self {
        Metadata {
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, name: &str, value: &str) -> Result<&mut Self, Status> {
        if name.starts_with(headers::RESREVER_NAME_PREFIX) {
            return Err(Status::new(Code::InvalidArgument, "invalid header name"));
        }

        for b in name.as_bytes() {
            if !ASCII_HEADER_NAME_TABLE[*b as usize] {
                return Err(Status::new(Code::InvalidArgument, "invalid header name"));
            }
        }

        for b in value.as_bytes() {
            if !ASCII_HEADER_VALUE_TABLE[*b as usize] {
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

    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.map.remove(name)
    }

    pub fn iter(&self) -> Iter {
        Iter(self.map.iter())
    }
}

impl std::ops::Index<&str> for Metadata {
    type Output = Vec<String>;

    fn index(&self, index: &str) -> &Self::Output {
        match self.map.get(index) {
            Some(s) => s,
            None => {
                panic!("Metadata[{}] did not exist", index)
            }
        }
    }
}

impl<'a> IntoIterator for &'a Metadata {
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

impl Default for Metadata {
    fn default() -> Self {
        Metadata::new()
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
