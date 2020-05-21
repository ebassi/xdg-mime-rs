use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use mime::Mime;

#[derive(Clone, PartialEq)]
pub struct Alias {
    pub alias: Mime,
    pub mime_type: Mime,
}

impl fmt::Debug for Alias {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Alias {} {}", self.alias, self.mime_type)
    }
}

impl Alias {
    pub fn new(alias: &Mime, mime_type: &Mime) -> Alias {
        Alias {
            alias: alias.clone(),
            mime_type: mime_type.clone(),
        }
    }

    pub fn from_string(s: &str) -> Option<Alias> {
        let mut chunks = s.split_whitespace().fuse();
        let alias = chunks.next().and_then(|s| Mime::from_str(s).ok())?;
        let mime_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;

        // Consume the leftovers, if any
        if chunks.next().is_some() {
            return None;
        }

        Some(Alias { alias, mime_type })
    }

    pub fn is_equivalent(&self, other: &Alias) -> bool {
        self.alias == other.alias
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
        self.aliases.sort_by(|a, b| a.alias.cmp(&b.alias))
    }

    pub fn unalias_mime_type(&self, mime_type: &Mime) -> Option<Mime> {
        self.aliases
            .iter()
            .find(|a| a.alias == *mime_type)
            .map(|a| a.mime_type.clone())
    }

    pub fn clear(&mut self) {
        self.aliases.clear();
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
        if line.is_err() {
            return res; // FIXME: return error instead
        }

        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Alias::from_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    res
}

pub fn read_aliases_from_dir<P: AsRef<Path>>(dir: P) -> Vec<Alias> {
    let mut alias_file = PathBuf::new();
    alias_file.push(dir);
    alias_file.push("aliases");

    read_aliases_from_file(alias_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_alias() {
        assert!(Alias::new(
            &Mime::from_str("application/foo").unwrap(),
            &Mime::from_str("application/foo").unwrap()
        )
        .is_equivalent(&Alias::new(
            &Mime::from_str("application/foo").unwrap(),
            &Mime::from_str("application/x-foo").unwrap()
        )),);
    }

    #[test]
    fn from_str() {
        assert_eq!(
            Alias::from_string("application/x-foo application/foo").unwrap(),
            Alias::new(
                &Mime::from_str("application/x-foo").unwrap(),
                &Mime::from_str("application/foo").unwrap(),
            )
        );
    }

    #[test]
    fn extra_tokens_yield_error() {
        assert!(Alias::from_string("one/foo two/foo three/foo").is_none());
    }
}
