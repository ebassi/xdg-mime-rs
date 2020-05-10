use std::cmp::Ordering;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

#[derive(Clone, Eq)]
pub struct Alias {
    pub alias: String,
    pub mime_type: String,
}

impl fmt::Debug for Alias {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Alias {} {}\n", self.alias, self.mime_type)
    }
}

impl PartialEq for Alias {
    fn eq(&self, other: &Alias) -> bool {
        self.alias == other.alias
    }
}

impl Ord for Alias {
    fn cmp(&self, other: &Alias) -> Ordering {
        self.alias.cmp(&other.alias)
    }
}

impl PartialOrd for Alias {
    fn partial_cmp(&self, other: &Alias) -> Option<Ordering> {
        Some(self.alias.cmp(&other.alias))
    }
}

impl Alias {
    pub fn new<S: Into<String>>(alias: S, mime_type: S) -> Alias {
        Alias {
            alias: alias.into(),
            mime_type: mime_type.into(),
        }
    }

    pub fn from_string(s: String) -> Option<Alias> {
        let mut chunks = s.split_whitespace();

        let alias = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        let mime_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        Some(Alias {
            alias: alias,
            mime_type: mime_type,
        })
    }
}

pub struct AliasesList {
    aliases: Vec<Alias>,
}

impl AliasesList {
    pub fn new() -> AliasesList {
        AliasesList {
            aliases: Vec::new(),
        }
    }

    pub fn add_alias(&mut self, alias: Alias) {
        self.aliases.push(alias);
    }

    pub fn add_aliases(&mut self, aliases: Vec<Alias>) {
        self.aliases.extend(aliases);
    }

    pub fn sort(&mut self) {
        self.aliases.sort_unstable();
    }

    pub fn unalias_mime_type(&self, mime_type: &str) -> Option<String> {
        for a in self.aliases.iter() {
            if a.alias == *mime_type {
                return Some(a.mime_type.to_string());
            }
        }

        None
    }
}

pub fn read_aliases_from_file<P: AsRef<Path>>(file_name: P) -> Vec<Alias> {
    let mut res = Vec::new();

    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return res,
    };

    let file = BufReader::new(&f);
    for line in file.lines() {
        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Alias::from_string(line) {
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
    fn new_alias() {
        assert_eq!(
            Alias::new("application/foo", "application/foo"),
            Alias::new("application/foo", "application/x-foo")
        );
    }

    #[test]
    fn from_str() {
        assert_eq!(
            Alias::from_string("application/x-foo application/foo".to_string()).unwrap(),
            Alias::new("application/x-foo", "application/foo")
        );
    }
}
