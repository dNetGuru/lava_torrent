//! Module for bencode-related parsing/encoding.
//!
//! Most of methods are associated methods of `BencodeElem`. Some general methods
//! are placed at the module level, and they can be found in [`write`](write/index.html).

use itertools;
use itertools::Itertools;
use std::collections::HashMap;
use std::convert::From;
use std::fmt;

#[cfg(test)]
#[macro_use]
mod macros;
mod read;
pub mod write;

const DICTIONARY_PREFIX: u8 = b'd';
const DICTIONARY_POSTFIX: u8 = b'e';
const LIST_PREFIX: u8 = b'l';
const LIST_POSTFIX: u8 = b'e';
const INTEGER_PREFIX: u8 = b'i';
const INTEGER_POSTFIX: u8 = b'e';
const STRING_DELIMITER: u8 = b':';

/// Represent a single bencode element.
///
/// There are 4 variants in the [spec], but this enum has 6 variants. The extra variants are
/// `Bytes` (a sequence of bytes that does not represent a valid utf8
/// string, e.g. a SHA1 block hash), which is considered to be the
/// same as `String` in the [spec], and `RawDictionary`, which has keys that are not
/// valid utf8 strings. They are best treated differently
/// in actual implementations to make things easier.
///
/// Note that the `Integer` variant here uses `i64` explicitly instead of using a type alias like
/// [`Integer`]. The reasoning behind this is that if you have to handle
/// bencode directly then what you are doing is relatively low-level. In this case, exposing the
/// underlying type might actually be better.
///
/// [`Integer`]: ../torrent/v1/type.Integer.html
/// [spec]: http://bittorrent.org/beps/bep_0003.html
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BencodeElem {
    String(String),
    Bytes(Vec<u8>),
    Integer(i64),
    List(Vec<BencodeElem>),
    Dictionary(HashMap<String, BencodeElem>),
    RawDictionary(HashMap<Vec<u8>, BencodeElem>),
}

impl From<u8> for BencodeElem {
    fn from(val: u8) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<u16> for BencodeElem {
    fn from(val: u16) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<u32> for BencodeElem {
    fn from(val: u32) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<i8> for BencodeElem {
    fn from(val: i8) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<i16> for BencodeElem {
    fn from(val: i16) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<i32> for BencodeElem {
    fn from(val: i32) -> BencodeElem {
        BencodeElem::Integer(i64::from(val))
    }
}

impl From<i64> for BencodeElem {
    fn from(val: i64) -> BencodeElem {
        BencodeElem::Integer(val)
    }
}

impl<'a> From<&'a str> for BencodeElem {
    fn from(val: &'a str) -> BencodeElem {
        BencodeElem::String(val.to_owned())
    }
}

impl From<String> for BencodeElem {
    fn from(val: String) -> BencodeElem {
        BencodeElem::String(val)
    }
}

impl<'a> From<&'a [u8]> for BencodeElem {
    fn from(val: &'a [u8]) -> BencodeElem {
        BencodeElem::Bytes(val.to_owned())
    }
}

impl From<Vec<u8>> for BencodeElem {
    fn from(val: Vec<u8>) -> BencodeElem {
        BencodeElem::Bytes(val)
    }
}

impl fmt::Display for BencodeElem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BencodeElem::String(ref string) => write!(f, r#""{}""#, string),
            BencodeElem::Bytes(ref bytes) => write!(f, "[{:#02x}]", bytes.iter().format(", ")),
            BencodeElem::Integer(ref int) => write!(f, "{}", int),
            BencodeElem::List(ref list) => write!(f, "[{}]", itertools::join(list, ", ")),
            BencodeElem::Dictionary(ref dict) => write!(
                f,
                "{{ {} }}",
                dict.iter()
                    .sorted_by_key(|&(key, _)| key.as_bytes())
                    .format_with(", ", |(k, v), f| f(&format_args!(r#"("{}", {})"#, k, v)))
            ),
            BencodeElem::RawDictionary(ref dict) => write!(
                f,
                "{{ {} }}",
                dict.iter()
                    .sorted_by_key(|&(key, _)| key)
                    .format_with(", ", |(k, v), f| f(&format_args!(
                        r#"("{}", {})"#,
                        k.iter().map(|b| format!("{:x}", b)).format(""),
                        v
                    )))
            ),
        }
    }
}

#[cfg(test)]
mod bencode_elem_display_tests {
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn display_test_string() {
        assert_eq!(bencode_elem!("").to_string(), r#""""#);
    }

    #[test]
    fn display_test_bytes() {
        assert_eq!(
            bencode_elem!((0xff, 0xf8, 0xff, 0xee)).to_string(),
            "[0xff, 0xf8, 0xff, 0xee]"
        );
    }

    #[test]
    fn display_test_integer() {
        assert_eq!(bencode_elem!(0).to_string(), "0");
    }

    #[test]
    fn display_test_list() {
        assert_eq!(bencode_elem!([0, "spam"]).to_string(), r#"[0, "spam"]"#);
    }

    #[test]
    fn display_test_dictionary() {
        assert_eq!(
            bencode_elem!({ ("cow", { ("moo", 4) }), ("spam", "eggs") }).to_string(),
            r#"{ ("cow", { ("moo", 4) }), ("spam", "eggs") }"#,
        )
    }
}
