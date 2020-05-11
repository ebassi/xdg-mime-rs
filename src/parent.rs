use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Clone, Eq)]
pub struct Subclass {
    mime_type: String,
    parent_type: String,
}

impl Subclass {
    pub fn new(mime_type: &str, parent_type: &str) -> Subclass {
        Subclass {
            mime_type: mime_type.to_string(),
            parent_type: parent_type.to_string(),
        }
    }

    fn from_string(s: &str) -> Option<Subclass> {
        let mut chunks = s.split_whitespace().fuse();
        let mime_type = chunks.next()?;
        let parent_type = chunks.next()?;

        // Consume the leftovers, if any
        if chunks.next().is_some() {
            return None;
        }

        Some(Subclass::new(mime_type, parent_type))
    }
}

impl fmt::Debug for Subclass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Subclass {} {}", self.parent_type, self.mime_type)
    }
}

impl PartialEq for Subclass {
    fn eq(&self, other: &Subclass) -> bool {
        self.parent_type == other.parent_type
    }
}

impl Ord for Subclass {
    fn cmp(&self, other: &Subclass) -> Ordering {
        self.parent_type.cmp(&other.parent_type)
    }
}

impl PartialOrd for Subclass {
    fn partial_cmp(&self, other: &Subclass) -> Option<Ordering> {
        Some(self.parent_type.cmp(&other.parent_type))
    }
}

pub struct ParentsMap {
    parents: HashMap<String, Vec<String>>,
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

    pub fn lookup(&self, mime_type: &str) -> Option<&Vec<String>> {
        self.parents.get(mime_type)
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
            Subclass::new("message/partial", "text/plain")
        );
    }

    #[test]
    fn parent_map() {
        let mut pm = ParentsMap::new();

        pm.add_subclass(Subclass::new("message/partial", "text/plain"));
        pm.add_subclass(Subclass::new("text/rfc822-headers", "text/plain"));

        assert_eq!(
            pm.lookup(&"message/partial".to_string()),
            Some(&vec!["text/plain".to_string(),])
        );
    }

    #[test]
    fn extra_tokens_yield_error() {
        assert!(Subclass::from_string("one/foo two/foo three/foo").is_none());
    }
}
