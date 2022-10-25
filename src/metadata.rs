use std::collections::BTreeMap;

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

pub enum Error {
    InvalidHeaderName,
    InvalidHeaderValue,
}

pub struct Metadata {
    pub(crate) map: BTreeMap<String, String>,
}

impl Metadata {
    pub fn new() -> Self {
        Metadata {
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, name: &str, value: &str) -> Result<&mut Self, Error> {
        for b in name.as_bytes() {
            if !ASCII_HEADER_NAME_TABLE[*b as usize] {
                return Err(Error::InvalidHeaderName);
            }
        }

        for b in value.as_bytes() {
            if !ASCII_HEADER_VALUE_TABLE[*b as usize] {
                return Err(Error::InvalidHeaderValue);
            }
        }

        self.map.insert(name.to_string(), value.to_string());

        Ok(self)
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
