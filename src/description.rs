use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

pub type Comments = HashMap<String, String>;
const MIME_NAMESPACE: &str = "http://www.freedesktop.org/standards/shared-mime-info";
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

fn language_key(language: &str) -> String {
    language.replace('-', "_").to_lowercase()
}

pub fn load(dirs: impl Iterator<Item = PathBuf>, mime: &mime::Mime) -> Comments {
    let mut comments = Comments::new();
    for dir in dirs {
        let path = dir.join(format!("{}.xml", mime.essence_str()));
        let mut xml = String::new();
        let Ok(file) = File::open(path) else { continue };
        // A type's generated XML is small. Do not allocate unbounded memory
        // for an accidentally corrupt local database file.
        if file.take(1024 * 1024).read_to_string(&mut xml).is_err() {
            continue;
        }
        let Ok(document) = roxmltree::Document::parse(&xml) else {
            continue;
        };
        let root = document.root_element();
        if !root.has_tag_name((MIME_NAMESPACE, "mime-type")) {
            continue;
        }
        for node in root
            .children()
            .filter(|n| n.has_tag_name((MIME_NAMESPACE, "comment")))
        {
            let language = language_key(node.attribute((XML_NAMESPACE, "lang")).unwrap_or(""));
            let text: String = node
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            if !text.trim().is_empty() {
                // Directories arrive in descending XDG precedence.
                comments
                    .entry(language)
                    .or_insert_with(|| text.trim().to_owned());
            }
        }
    }
    comments
}

pub fn lookup(comments: &Comments, languages: &[&str]) -> Option<String> {
    for language in languages {
        let (locale, modifier) = language.split_once('@').unwrap_or((language, ""));
        let locale = language_key(locale.split('.').next().unwrap_or(locale));
        if locale == "c" || locale == "posix" {
            break;
        }
        let base = locale.split('_').next().unwrap_or(&locale);
        let mut candidates = Vec::new();
        if !modifier.is_empty() {
            candidates.push(format!("{locale}@{}", modifier.to_lowercase()));
        }
        candidates.push(locale.clone());
        if !modifier.is_empty() {
            candidates.push(format!("{base}@{}", modifier.to_lowercase()));
        }
        candidates.push(base.to_owned());
        for candidate in candidates {
            if let Some(comment) = comments.get(&candidate) {
                return Some(comment.clone());
            }
        }
    }
    comments.get("").cloned()
}
