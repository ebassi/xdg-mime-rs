use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use mime::Mime;

#[derive(Clone, PartialEq)]
pub struct Icon {
    icon_name: String,
    mime_type: Mime,
}

impl fmt::Debug for Icon {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Icon for {}: {}", self.mime_type, self.icon_name)
    }
}

impl Icon {
    pub fn new(icon_name: &str, mime_type: &Mime) -> Icon {
        Icon {
            icon_name: icon_name.to_string(),
            mime_type: mime_type.clone(),
        }
    }

    pub fn from_string(s: &str) -> Option<Icon> {
        let mut chunks = s.split(':').fuse();
        let mime_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;
        let icon_name = chunks.next().filter(|s| !s.is_empty())?;

        // Consume the leftovers, if any
        if chunks.next().is_some() {
            return None;
        }

        Some(Icon {
            icon_name: icon_name.to_string(),
            mime_type
        })
    }
}

pub fn read_icons_from_file<P: AsRef<Path>>(file_name: P) -> Vec<Icon> {
    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut res = Vec::new();
    let file = BufReader::new(&f);
    for line in file.lines() {
        if line.is_err() {
            return res; // FIXME: return error instead
        }

        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Icon::from_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    res.sort_by(|a, b| a.mime_type.cmp(&b.mime_type));

    res
}

pub fn read_icons_from_dir<P: AsRef<Path>>(dir: P, generic: bool) -> Vec<Icon> {
    let mut icons_file = PathBuf::new();
    icons_file.push(dir);

    if generic {
        icons_file.push("generic-icons");
    } else {
        icons_file.push("icons");
    }

    read_icons_from_file(icons_file)
}

pub fn find_icon(icons: &[Icon], mime_type: &Mime) -> Option<String> {
    for icon in icons {
        if icon.mime_type == *mime_type {
            return Some(icon.icon_name.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str() {
        assert_eq!(
            Icon::from_string("application/rss+xml:text-html").unwrap(),
            Icon::new("text-html", &Mime::from_str("application/rss+xml").unwrap())
        );
    }

    #[test]
    fn from_str_catches_syntax_error() {
        assert!(Icon::from_string("one:two:three").is_none());
        assert!(Icon::from_string(":").is_none());
        assert!(Icon::from_string("one:").is_none());
        assert!(Icon::from_string(":two").is_none());
        assert!(Icon::from_string("").is_none());
    }
}
