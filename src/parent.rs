use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use mime::Mime;

#[derive(Clone, PartialEq)]
pub struct Subclass {
    mime_type: Mime,
    parent_type: Mime,
}

impl Subclass {
    pub fn new(mime_type: &Mime, parent_type: &Mime) -> Subclass {
        Subclass {
            mime_type: mime_type.clone(),
            parent_type: parent_type.clone(),
        }
    }

    fn from_string(s: &str) -> Option<Subclass> {
        let mut chunks = s.split_whitespace().fuse();
        let mime_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;
        let parent_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;

        // Consume the leftovers, if any
        if chunks.next().is_some() {
            return None;
        }

        Some(Subclass {
            mime_type,
            parent_type,
        })
    }
}

impl fmt::Debug for Subclass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Subclass {} {}", self.parent_type, self.mime_type)
    }
}

pub struct ParentsMap {
    parents: HashMap<Mime, Vec<Mime>>,
}

impl ParentsMap {
    pub fn new() -> ParentsMap {
        ParentsMap {
            parents: HashMap::new(),
        }
    }

    fn add_subclass(&mut self, subclass: Subclass) {
        let v = self
            .parents
            .entry(subclass.mime_type.clone())
            .or_insert_with(Vec::new);
        if !v.contains(&subclass.parent_type) {
            v.push(subclass.parent_type);
        }
    }

    pub fn add_subclasses(&mut self, subclasses: Vec<Subclass>) {
        for s in subclasses {
            self.add_subclass(s);
        }
    }

    pub fn lookup(&self, mime_type: &Mime) -> Option<&Vec<Mime>> {
        self.parents.get(mime_type)
    }

    pub fn clear(&mut self) {
        self.parents.clear();
    }
}

pub fn read_subclasses_from_file<P: AsRef<Path>>(file_name: P) -> Vec<Subclass> {
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

        match Subclass::from_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    res
}

pub fn read_subclasses_from_dir<P: AsRef<Path>>(dir: P) -> Vec<Subclass> {
    let mut subclasses_file = PathBuf::new();
    subclasses_file.push(dir);
    subclasses_file.push("subclasses");

    read_subclasses_from_file(subclasses_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str() {
        assert_eq!(
            Subclass::from_string("message/partial text/plain").unwrap(),
            Subclass::new(
                &Mime::from_str("message/partial").unwrap(),
                &Mime::from_str("text/plain").unwrap()
            )
        );
    }

    #[test]
    fn parent_map() {
        let mut pm = ParentsMap::new();

        pm.add_subclass(Subclass::new(
            &Mime::from_str("message/partial").unwrap(),
            &Mime::from_str("text/plain").unwrap(),
        ));
        pm.add_subclass(Subclass::new(
            &Mime::from_str("text/rfc822-headers").unwrap(),
            &Mime::from_str("text/plain").unwrap(),
        ));

        assert_eq!(
            pm.lookup(&Mime::from_str("message/partial").unwrap()),
            Some(&vec![Mime::from_str("text/plain").unwrap()]),
        );
    }

    #[test]
    fn extra_tokens_yield_error() {
        assert!(Subclass::from_string("one/foo two/foo three/foo").is_none());
    }
}
