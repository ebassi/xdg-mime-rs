use nom::bytes::complete::take_until;
use nom::character::complete::line_ending;
use nom::character::is_hex_digit;
use nom::combinator::map_res;
use nom::number::streaming::be_u16;
use nom::IResult;
use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};
use std::vec::Vec;

use mime::Mime;

pub fn to_string(s: &[u8]) -> std::result::Result<&str, std::str::Utf8Error> {
    str::from_utf8(s)
}

pub fn to_u32(s: std::result::Result<&str, std::str::Utf8Error>, or_default: u32) -> u32 {
    match s {
        Ok(t) => str::FromStr::from_str(t).unwrap_or(or_default),
        Err(_) => or_default,
    }
}

pub fn buf_to_u32(s: &[u8], or_default: u32) -> u32 {
    to_u32(to_string(s), or_default)
}

#[derive(Clone, Debug, PartialEq)]
struct MagicRule {
    indent: u32,
    start_offset: u32,
    value_length: u16,
    value: Vec<u8>,
    mask: Option<Vec<u8>>,
    word_size: u32,
    range_length: u32,
}

impl MagicRule {
    fn matches_data(&self, data: &[u8]) -> bool {
        let start: usize = self.start_offset as usize;
        let end: usize = self.start_offset as usize + self.range_length as usize;

        for i in start..end {
            let mut res: bool = true;

            let value_len: usize = self.value_length as usize;

            if i + value_len > data.len() {
                return false;
            }

            match &self.mask {
                Some(m) => {
                    for j in 0..value_len {
                        let masked_value = self.value[j] & m[j];
                        let masked_data = data[j + i] & m[j];
                        if masked_value != masked_data {
                            res = false;
                            break;
                        }
                    }
                }
                None => {
                    for j in 0..value_len {
                        if data[j + i] != self.value[j] {
                            res = false;
                            break;
                        }
                    }
                }
            }

            if res {
                return true;
            }
        }

        false
    }

    fn extent(&self) -> usize {
        let value_len = self.value_length as usize;
        let offset = self.start_offset as usize;
        let range_len = self.range_length as usize;

        value_len + offset + range_len
    }
}

// Indentation level, can be 0
named!(
    indent_level<u32>,
    do_parse!(
        res: take_until!(">")
    >>  (buf_to_u32(res, 0))
    )
);

// Offset, can be 0
named!(
    start_offset<u32>,
    do_parse!(
        res: take_until!("=")
    >>  (buf_to_u32(res, 0))
    )
);


// <word_size> = '~' (0 | 1 | 2 | 4)
named!(
    word_size<Option<u32>>,
    opt!(
        do_parse!(
            tag!("~")
        >>  res: alt!(
                tag!("0") | tag!("1") | tag!("2") | tag!("4")
            )
        >>  (buf_to_u32(res, 1))
	)
    )
);

// <range_length> = '+' <u32>
named!(
    range_length<Option<u32>>,
    opt!(
        do_parse!(
            tag!("+")
        >>  res: take_while!(is_hex_digit)
        >>  (buf_to_u32(res, 1))
	)
    )
);

// magic_rule =
// [ <indent> ] '>' <start-offset> '=' <value_length> <value>
// [ '&' <mask> ] [ <word_size> ] [ <range_length> ]
// '\n'
named!(
    magic_rule<MagicRule>,
    do_parse!(
        peek!(is_a!("0123456789>"))
    >>  _indent: indent_level
    >>  tag!(">")
    >>  _start_offset: start_offset
    >>  tag!("=")
    >>  _value_length: be_u16
    >>  _value: do_parse!(
            res: take!(_value_length)
        >>  (res.iter().copied().collect())
        )
    >>  _mask: opt!(
            do_parse!(
                char!('&')
            >>  res: take!(_value_length)
            >>  (res.iter().copied().collect())
	    )
        )
    >>  _word_size: word_size
    >>  _range_length: range_length
    >>  line_ending
    >>  (MagicRule {
            indent: _indent,
            start_offset: _start_offset,
            value_length: _value_length,
            value: _value,
            mask: _mask,
            word_size: _word_size.unwrap_or(1),
            range_length: _range_length.unwrap_or(1),
        })
    )
);

#[derive(Clone, PartialEq)]
pub struct MagicEntry {
    mime_type: Mime,
    priority: u32,
    rules: Vec<MagicRule>,
}

impl fmt::Debug for MagicEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MIME type: {:?} (priority: {:?}):\nrules:\n{:?}",
            self.mime_type, self.priority, self.rules
        )
    }
}

impl MagicEntry {
    fn matches(&self, data: &[u8]) -> Option<(&Mime, u32)> {
        let mut current_level = 0;

        let mut iter = (&self.rules).iter().peekable();
        while let Some(rule) = iter.next() {
            // The rules are a flat list that represent a tree; the "indent"
            // is the depth of the rule in the tree. If a rule matches at a
            // certain level, we increase the level and iterate to the next
            // rule at that level. If this is the last rule, we traversed the
            // branch; otherwise, we go back one level and keep matching.
            if rule.indent == current_level {
                if rule.matches_data(data) {
                    current_level += 1;
                    match iter.peek() {
                        Some(next) => {
                            // go back one level
                            if next.indent < current_level {
                                current_level -= 1;
                            }
                        }
                        None => {
                            // last rule
                            return Some((&self.mime_type, self.priority));
                        }
                    };
                } else {
                    // No match at the current level, start from scratch
                    current_level = 0;
                }
            }
        }

        None
    }

    fn max_extents(&self) -> usize {
        let mut res: usize = 0;
        for rule in &self.rules {
            let rule_extent = rule.extent();
            if rule_extent > res {
                res = rule_extent;
            }
        }

        res
    }
}

named!(priority<u32>,
    do_parse!(
        res: take_until!(":")
    >>  (buf_to_u32(res, 0))
    )
);

fn mime_type(bytes: &[u8]) -> IResult<&[u8], Mime> {
    map_res(
        map_res(
            take_until("]\n"),
            str::from_utf8
        ),
        Mime::from_str
    )(bytes)
}

// magic_header =
// '[' <priority> ':' <mime_type> ']' '\n'
named!(magic_header<(u32, Mime)>,
    do_parse!(
        tag!("[")
    >>  _priority: priority
    >>  tag!(":")
    >>  _mime_type: mime_type
    >>  tag!("]\n")
    >>  (_priority, _mime_type)
    )
);

// magic_entry =
// <magic_header>
// <magic_rule>+
named!(magic_entry<MagicEntry>,
    do_parse!(
        _header: magic_header
    >>  _rules: many1!(complete!(magic_rule))
    >>  (MagicEntry {
            priority: _header.0,
            mime_type: _header.1,
            rules: _rules,
        })
    )
);

named!(from_u8_to_entries<Vec<MagicEntry>>,
    do_parse!(
	tag!("MIME-Magic\0\n")
    >>  res: many0!(complete!(magic_entry))
    >>  (res)
    )
);

pub fn lookup_data(entries: &[MagicEntry], data: &[u8]) -> Option<(Mime, u32)> {
    for entry in entries {
        if let Some(v) = entry.matches(data) {
            return Some((v.0.clone(), v.1));
        }
    }

    None
}

pub fn max_extents(entries: &[MagicEntry]) -> usize {
    let mut res: usize = 0;
    for entry in entries {
        let extents = entry.max_extents();
        if extents > res {
            res = extents;
        }
    }

    res
}

pub fn read_magic_from_file<P: AsRef<Path>>(file_name: P) -> Vec<MagicEntry> {
    let mut f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut magic_buf = Vec::<u8>::new();

    f.read_to_end(&mut magic_buf).unwrap();
    match from_u8_to_entries(magic_buf.as_slice()) {
        Ok(v) => v.1,
        Err(_) => Vec::new(),
    }
}

pub fn read_magic_from_dir<P: AsRef<Path>>(dir: P) -> Vec<MagicEntry> {
    let mut magic_file = PathBuf::new();
    magic_file.push(dir);
    magic_file.push("magic");

    read_magic_from_file(magic_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::HexDisplay;
    use nom::Offset;

    #[test]
    fn parse_magic_header() {
        let res = magic_header(&"[50:application/x-yaml]\n".as_bytes());

        match res {
            Ok((i, o)) => {
                assert_eq!(i.len(), 0);
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic rule");
            }
        }
    }

    #[test]
    fn parse_one_magic_rule() {
        let simple = include_bytes!("../test_files/parser/single_rule");
        println!("bytes:\n{}", &simple.to_hex(8));
        let simple_res = magic_rule(simple);

        match simple_res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, simple.offset(i)));
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic rule");
            }
        }

        let range = include_bytes!("../test_files/parser/rule_with_range");
        println!("bytes:\n{}", &range.to_hex(8));
        let range_res = magic_rule(range);

        match range_res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, range.offset(i)));
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic rule");
            }
        }

        let ws = include_bytes!("../test_files/parser/rule_with_ws");
        println!("bytes:\n{}", &ws.to_hex(8));
        let ws_res = magic_rule(ws);

        match ws_res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, ws.offset(i)));
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic rule");
            }
        }
    }

    #[test]
    fn parse_simple_magic_entry() {
        let data = include_bytes!("../test_files/parser/single_entry");
        println!("bytes:\n{}", &data.to_hex(8));
        let res = magic_entry(data);

        match res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, data.offset(i)));
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic entry");
            }
        }
    }

    #[test]
    fn parse_magic_entry() {
        let data = include_bytes!("../test_files/parser/many_rules");
        println!("bytes:\n{}", &data.to_hex(8));
        let res = magic_entry(data);

        match res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, data.offset(i)));
                println!("parsed:\n{:?}", o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic entry");
            }
        }
    }

    #[test]
    fn parse_magic_file() {
        let data = include_bytes!("../test_files/mime/magic");
        let res = from_u8_to_entries(data);

        match res {
            Ok((i, o)) => {
                println!("remaining:\n{}", &i.to_hex_from(8, data.offset(i)));
                println!("parsed {} magic entries:\n{:#?}", o.len(), o);
            }
            e => {
                println!("invalid or incomplete: {:?}", e);
                panic!("cannot parse magic file");
            }
        }
    }
}
