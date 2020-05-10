use std::cmp::Ordering;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Clone, Eq)]
pub struct Icon {
    icon_name: String,
    mime_type: String,
}

impl fmt::Debug for Icon {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Icon for {}: {}", self.mime_type, self.icon_name)
    }
}

impl PartialEq for Icon {
    fn eq(&self, other: &Icon) -> bool {
        self.mime_type == other.mime_type
    }
}

impl Ord for Icon {
    fn cmp(&self, other: &Icon) -> Ordering {
        self.mime_type.cmp(&other.mime_type)
    }
}

impl PartialOrd for Icon {
    fn partial_cmp(&self, other: &Icon) -> Option<Ordering> {
        Some(self.mime_type.cmp(&other.mime_type))
    }
}

impl Icon {
    pub fn new<S: Into<String>>(icon_name: S, mime_type: S) -> Icon {
        Icon {
            icon_name: icon_name.into(),
            mime_type: mime_type.into(),
        }
    }

    pub fn from_string(s: &str) -> Option<Icon> {
        let mut chunks = s.split(':');

        let mime_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        let icon_name = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        if icon_name.is_empty() || mime_type.is_empty() {
            return None;
        }

        if chunks.count() != 0 {
            return None;
        }

        Some(Icon {
            icon_name,
            mime_type,
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
        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Icon::from_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    res.sort_unstable();

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

pub fn find_icon(icons: &Vec<Icon>, mime_type: &str) -> Option<String> {
    for icon in icons {
        if icon.mime_type == mime_type {
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
            Icon::new("text-html", "application/rss+xml")
        );
    }
}
