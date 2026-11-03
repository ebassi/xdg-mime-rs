use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use glob::Pattern;
use mime::Mime;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum GlobType {
    Literal(String),
    Simple(String),
    Full(Pattern),
    Invalid(String),
    DeleteAll,
}

impl fmt::Debug for GlobType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GlobType::Literal(name) => write!(f, "Literal '{}'", name),
            GlobType::Simple(pattern) => write!(f, "Simple glob '*{}'", pattern),
            GlobType::Full(pattern) => write!(f, "Full glob '{}'", pattern),
            GlobType::Invalid(pattern) => write!(f, "Invalid glob '{}'", pattern),
            GlobType::DeleteAll => write!(f, "Delete all globs"),
        }
    }
}

impl fmt::Display for GlobType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GlobType::Literal(str) => write!(f, "{}", str),
            GlobType::Simple(str) => write!(f, "{}", str),
            GlobType::Full(pattern) => write!(f, "{}", pattern),
            GlobType::Invalid(pattern) => write!(f, "{}", pattern),
            GlobType::DeleteAll => write!(f, "__NOGLOBS__"),
        }
    }
}

fn determine_type(glob: &str) -> GlobType {
    let mut maybe_simple = false;

    for (idx, ch) in glob.bytes().enumerate() {
        if idx == 0 && ch == b'*' {
            maybe_simple = true;
        } else if ch == b'\\' || ch == b'[' || ch == b'*' || ch == b'?' {
            return Pattern::new(glob)
                .map(GlobType::Full)
                .unwrap_or_else(|_| GlobType::Invalid(glob.to_owned()));
        }
    }

    if maybe_simple {
        GlobType::Simple(glob[1..].to_string())
    } else {
        GlobType::Literal(glob.to_string())
    }
}

#[derive(Clone)]
pub struct Glob {
    glob: GlobType,
    weight: i32,
    case_sensitive: bool,
    mime_type: Mime,
    length: usize,
}

impl PartialEq for Glob {
    fn eq(&self, other: &Glob) -> bool {
        self.glob == other.glob
            && self.mime_type == other.mime_type
            && self.case_sensitive == other.case_sensitive
    }
}

impl Eq for Glob {}

impl Hash for Glob {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.glob.hash(h);
        self.mime_type.hash(h);
        self.case_sensitive.hash(h)
    }
}

impl fmt::Debug for Glob {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Glob: {:?} {:?} (weight: {}, cs: {})",
            self.glob, self.mime_type, self.weight, self.case_sensitive
        )
    }
}

impl Glob {
    pub fn simple(mime_type: &Mime, glob: &str) -> Glob {
        Self::new(mime_type, glob, 50, false)
    }

    pub fn with_weight(mime_type: &Mime, glob: &str, weight: i32) -> Glob {
        Self::new(mime_type, glob, weight, false)
    }

    pub fn new(mime_type: &Mime, glob: &str, weight: i32, cs: bool) -> Glob {
        let pattern = if glob == "__NOGLOBS__" {
            GlobType::DeleteAll
        } else if cs {
            determine_type(glob)
        } else {
            determine_type(&glob.to_lowercase())
        };
        Glob {
            mime_type: mime_type.clone(),
            glob: pattern,
            weight,
            case_sensitive: cs,
            length: glob.len(),
        }
    }

    pub fn from_v1_string(s: &str) -> Option<Glob> {
        let mut chunks = s.split(':').fuse();
        let mime_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;
        let glob = chunks.next().filter(|s| !s.is_empty())?;

        // The globs file is not extensible, so consume any
        // leftover tokens
        if chunks.next().is_some() {
            return None;
        }

        let glob = Glob::new(&mime_type, glob, 50, false);
        (!matches!(glob.glob, GlobType::Invalid(_))).then_some(glob)
    }

    pub fn from_v2_string(s: &str) -> Option<Glob> {
        let mut chunks = s.split(':').fuse();

        let weight = chunks
            .next()
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|n| *n >= 0)?;

        let mime_type = chunks.next().and_then(|s| Mime::from_str(s).ok())?;
        let glob = chunks.next().filter(|s| !s.is_empty())?;

        let mut case_sensitive = false;
        if let Some(flags) = chunks.next() {
            let flags_chunks = flags.split(',').collect::<Vec<&str>>();

            // Allow for extra flags
            if flags_chunks.iter().any(|&f| f == "cs") {
                case_sensitive = true;
            }
        }

        // Ignore any other token, for extensibility:
        //
        // https://specifications.freedesktop.org/shared-mime-info-spec/shared-mime-info-spec-latest.html#idm46152099256048

        let glob = Glob::new(&mime_type, glob, weight, case_sensitive);
        (!matches!(glob.glob, GlobType::Invalid(_))).then_some(glob)
    }

    fn compare(&self, file_name: &str) -> bool {
        if self.case_sensitive {
            self.compare_normalized(file_name)
        } else {
            self.compare_normalized(&file_name.to_lowercase())
        }
    }

    fn compare_normalized(&self, file_name: &str) -> bool {
        match &self.glob {
            GlobType::Literal(s) => s == file_name,
            GlobType::Simple(s) => file_name.ends_with(s),
            GlobType::Full(p) => p.matches(file_name),
            GlobType::Invalid(_) | GlobType::DeleteAll => false,
        }
    }
}

pub fn read_globs_v1_from_file<P: AsRef<Path>>(file_name: P) -> Option<Vec<Glob>> {
    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut res = Vec::new();
    let file = BufReader::new(&f);
    for line in file.lines() {
        if line.is_err() {
            return None;
        }

        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Glob::from_v1_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    Some(res)
}

pub fn read_globs_v2_from_file<P: AsRef<Path>>(file_name: P) -> Option<Vec<Glob>> {
    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut res = Vec::new();
    let file = BufReader::new(&f);
    for line in file.lines() {
        if line.is_err() {
            return None;
        }

        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Glob::from_v2_string(&line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    Some(res)
}

pub fn read_globs_from_dir<P: AsRef<Path>>(dir: P) -> Vec<Glob> {
    let mut globs_file = PathBuf::new();
    globs_file.push(dir);
    globs_file.push("globs2");

    match read_globs_v2_from_file(&globs_file) {
        Some(v) => v,
        None => {
            globs_file.pop();
            globs_file.push("globs");

            read_globs_v1_from_file(globs_file).unwrap_or_default()
        }
    }
}

#[derive(Default)]
pub struct GlobMap {
    globs: HashSet<Glob>,
    literals: HashMap<String, Vec<Glob>>,
    suffixes: HashMap<String, Vec<Glob>>,
    full: Vec<Glob>,
    disabled: HashSet<Mime>,
}

impl GlobMap {
    pub fn new() -> GlobMap {
        Self::default()
    }

    pub fn add_glob(&mut self, glob: Glob) {
        if self.disabled.contains(&glob.mime_type) || !self.globs.insert(glob.clone()) {
            return;
        }
        match &glob.glob {
            GlobType::Literal(s) => self.literals.entry(s.clone()).or_default().push(glob),
            GlobType::Simple(s) => self.suffixes.entry(s.clone()).or_default().push(glob),
            GlobType::Full(_) => self.full.push(glob),
            GlobType::Invalid(_) | GlobType::DeleteAll => (),
        }
    }

    // Each batch belongs to one directory, in descending XDG precedence.
    // Activate markers after adding this directory's replacement patterns.
    pub fn add_globs(&mut self, globs: &[Glob]) {
        for glob in globs {
            self.add_glob(glob.clone());
        }
        for glob in globs {
            if matches!(glob.glob, GlobType::DeleteAll) {
                self.disabled.insert(glob.mime_type.clone());
            }
        }
    }

    pub fn lookup_mime_type_for_file_name(&self, file_name: &str) -> Option<Vec<Mime>> {
        let folded = file_name.to_lowercase();
        let mut matches = Vec::new();
        for (name, cs) in [(file_name, true), (folded.as_str(), false)] {
            if let Some(globs) = self.literals.get(name) {
                matches.extend(globs.iter().filter(|g| g.case_sensitive == cs));
            }
        }
        // Literal patterns take precedence over wildcard patterns.
        if matches.is_empty() {
            for (name, cs) in [(file_name, true), (folded.as_str(), false)] {
                for offset in name.char_indices().map(|(i, _)| i).chain([name.len()]) {
                    if let Some(globs) = self.suffixes.get(&name[offset..]) {
                        matches.extend(globs.iter().filter(|g| g.case_sensitive == cs));
                    }
                }
            }
            matches.extend(self.full.iter().filter(|g| {
                g.compare_normalized(if g.case_sensitive { file_name } else { &folded })
            }));
        }
        let best = matches.iter().map(|g| (g.weight, g.length)).max()?;
        let mut types: Vec<_> = matches
            .into_iter()
            .filter(|g| (g.weight, g.length) == best)
            .map(|g| g.mime_type.clone())
            .collect();
        // Overlapping patterns can name the same type. Stable ordering also
        // makes an ambiguous guess independent of HashSet iteration order.
        types.sort();
        types.dedup();
        Some(types)
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

impl fmt::Debug for GlobMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_set().entries(&self.globs).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(lines: &[&str]) -> Vec<Glob> {
        lines
            .iter()
            .filter_map(|line| Glob::from_v2_string(line))
            .collect()
    }

    #[test]
    fn overrides_only_remove_lower_priority_patterns() {
        let mut map = GlobMap::new();
        map.add_globs(&rules(&["50:text/plain:*.highest"]));
        map.add_globs(&rules(&[
            "0:text/plain:__NOGLOBS__",
            "50:text/plain:*.replacement",
            "0:image/png:__NOGLOBS__",
        ]));
        map.add_globs(&rules(&[
            "50:text/plain:README",
            "50:text/plain:*.old",
            "50:text/plain:old-*.txt",
            "50:image/png:*.png",
            "50:application/pdf:README",
        ]));
        for name in ["x.highest", "x.replacement"] {
            assert_eq!(
                map.lookup_mime_type_for_file_name(name),
                Some(vec![mime::TEXT_PLAIN])
            );
        }
        for name in ["x.old", "old-report.txt", "x.png", "__NOGLOBS__"] {
            assert_eq!(map.lookup_mime_type_for_file_name(name), None, "{name}");
        }
        assert_eq!(
            map.lookup_mime_type_for_file_name("README"),
            Some(vec![mime::APPLICATION_PDF])
        );
        map.clear();
        map.add_globs(&rules(&["50:text/plain:*.old"]));
        assert!(map.lookup_mime_type_for_file_name("x.old").is_some());
    }

    #[test]
    fn all_pattern_kinds_respect_case_and_malformed_patterns_are_skipped() {
        for pattern in ["ExactCase", "*.UPPER", "REPORT-*.TXT", "猫-?.TXT"] {
            let name = pattern.replace('*', "file").replace('?', "字");
            let cs = Glob::new(&mime::TEXT_PLAIN, pattern, 50, true);
            let ci = Glob::new(&mime::TEXT_PLAIN, pattern, 50, false);
            assert!(cs.compare(&name));
            assert!(!cs.compare(&name.to_lowercase()));
            assert!(ci.compare(&name));
            assert!(ci.compare(&name.to_lowercase()));
        }
        assert!(Glob::from_v2_string("50:text/plain:[").is_none());
        assert!(Glob::from_v1_string("text/plain:[").is_none());
        assert!(Glob::from_v2_string("50:text/plain:").is_none());
    }

    #[test]
    fn literal_precedence_weight_length_and_unique_stable_results() {
        let mut map = GlobMap::new();
        map.add_globs(&rules(&[
            "10:text/plain:README",
            "90:image/png:R*",
            "80:application/gzip:*.gz",
            "20:application/x-tar:*.tar.gz",
            "50:text/plain:*.abc",
            "50:text/plain:*.a?c",
            "50:image/png:*.abc",
        ]));
        assert_eq!(
            map.lookup_mime_type_for_file_name("README"),
            Some(vec![mime::TEXT_PLAIN])
        );
        assert_eq!(
            map.lookup_mime_type_for_file_name("x.tar.gz"),
            Some(vec!["application/gzip".parse().unwrap()])
        );
        assert_eq!(
            map.lookup_mime_type_for_file_name("x.abc"),
            Some(vec![mime::IMAGE_PNG, mime::TEXT_PLAIN])
        );
    }

    #[test]
    fn glob_type() {
        assert_eq!(
            determine_type("*.gif"),
            GlobType::Simple(".gif".to_string())
        );
        assert_eq!(
            determine_type("Foo*.gif"),
            GlobType::Full(Pattern::new("Foo*.gif").unwrap())
        );
        assert_eq!(
            determine_type("*[4].gif"),
            GlobType::Full(Pattern::new("*[4].gif").unwrap())
        );
        assert_eq!(
            determine_type("Makefile"),
            GlobType::Literal("Makefile".to_string())
        );
        assert_eq!(
            determine_type("sldkfjvlsdf\\\\slkdjf"),
            GlobType::Full(Pattern::new("sldkfjvlsdf\\\\slkdjf").unwrap())
        );
        assert_eq!(
            determine_type("tree.[ch]"),
            GlobType::Full(Pattern::new("tree.[ch]").unwrap())
        );
    }

    #[test]
    fn glob_v1_string() {
        assert_eq!(
            Glob::from_v1_string("text/rust:*.rs"),
            Some(Glob::simple(&Mime::from_str("text/rust").unwrap(), "*.rs"))
        );
        assert_eq!(
            Glob::from_v1_string("text/rust:*.rs"),
            Some(Glob::new(
                &Mime::from_str("text/rust").unwrap(),
                "*.rs",
                50,
                false
            ))
        );

        assert_eq!(Glob::from_v1_string(""), None);
        assert_eq!(Glob::from_v1_string("foo"), None);
        assert_eq!(Glob::from_v1_string("foo:"), None);
        assert_eq!(Glob::from_v1_string(":bar"), None);
        assert_eq!(Glob::from_v1_string(":"), None);
        assert_eq!(Glob::from_v1_string("foo:bar:baz"), None);
    }

    #[test]
    fn glob_v2_string() {
        assert_eq!(
            Glob::from_v2_string("80:text/rust:*.rs"),
            Some(Glob::with_weight(
                &Mime::from_str("text/rust").unwrap(),
                "*.rs",
                80
            ))
        );
        assert_eq!(
            Glob::from_v2_string("80:text/rust:*.rs"),
            Some(Glob::new(
                &Mime::from_str("text/rust").unwrap(),
                "*.rs",
                80,
                false
            ))
        );
        assert_eq!(
            Glob::from_v2_string("50:text/x-c++src:*.C:cs"),
            Some(Glob::new(
                &Mime::from_str("text/x-c++src").unwrap(),
                "*.C",
                50,
                true
            ))
        );

        assert_eq!(Glob::from_v2_string(""), None);
        assert_eq!(Glob::from_v2_string("foo"), None);
        assert_eq!(Glob::from_v2_string("foo:"), None);
        assert_eq!(Glob::from_v2_string(":bar"), None);
        assert_eq!(Glob::from_v2_string(":"), None);
        assert_eq!(Glob::from_v2_string("foo:bar:baz"), None);
        assert_eq!(Glob::from_v2_string("foo:bar:baz:blah"), None);

        assert_eq!(
            Glob::from_v2_string("50:text/x-c++src:*.C:cs,newflag:newfeature:somethingelse"),
            Some(Glob::new(
                &Mime::from_str("text/x-c++src").unwrap(),
                "*.C",
                50,
                true
            ))
        );
    }

    #[test]
    fn compare() {
        // Literal
        let copying = Glob::new(
            &Mime::from_str("text/x-copying").unwrap(),
            "copying",
            50,
            false,
        );
        assert_eq!(copying.compare(&"COPYING".to_string()), true);

        // Simple, case-insensitive
        let c_src = Glob::new(&Mime::from_str("text/x-csrc").unwrap(), "*.c", 50, false);
        assert_eq!(c_src.compare(&"foo.c".to_string()), true);
        assert_eq!(c_src.compare(&"FOO.C".to_string()), true);

        // Simple, case-sensitive
        let cplusplus_src = Glob::new(&Mime::from_str("text/x-c++src").unwrap(), "*.C", 50, true);
        assert_eq!(cplusplus_src.compare(&"foo.C".to_string()), true);
        assert_eq!(cplusplus_src.compare(&"foo.c".to_string()), false);
        assert_eq!(cplusplus_src.compare(&"foo.h".to_string()), false);

        // Full
        let video_x_anim = Glob::new(
            &Mime::from_str("video/x-anim").unwrap(),
            "*.anim[1-9j]",
            50,
            false,
        );
        assert_eq!(video_x_anim.compare(&"foo.anim0".to_string()), false);
        assert_eq!(video_x_anim.compare(&"foo.anim8".to_string()), true);
        assert_eq!(video_x_anim.compare(&"foo.animk".to_string()), false);
        assert_eq!(video_x_anim.compare(&"foo.animj".to_string()), true);
    }
}
