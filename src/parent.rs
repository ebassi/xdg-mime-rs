use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

#[derive(Clone, Eq)]
pub struct Subclass {
    mime_type: String,
    parent_type: String,
}

impl Subclass {
    pub fn new<S: Into<String>>(mime_type: S, parent_type: S) -> Subclass {
        Subclass {
            mime_type: mime_type.into(),
            parent_type: parent_type.into(),
        }
    }

    fn from_string(s: String) -> Option<Subclass> {
        let mut chunks = s.split_whitespace();

        let mime_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        let parent_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

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
            .or_insert(Vec::new());
        if !v.contains(&subclass.parent_type) {
            v.push(subclass.parent_type.clone());
        }
    }

    pub fn add_subclasses(&mut self, subclasses: Vec<Subclass>) {
        for s in subclasses {
            self.add_subclass(s);
        }
    }

    pub fn lookup<S: Into<String>>(&self, mime_type: S) -> Option<&Vec<String>> {
        let mime_type = mime_type.into();

        self.parents.get(&mime_type)
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

        match Subclass::from_string(line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str() {
        assert_eq!(
            Subclass::from_string("message/partial text/plain".to_string()).unwrap(),
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
}
