#![cfg(any(unix, target_os = "redox"))]

// FIXME: Remove once we test everything
#![allow(dead_code)]

/// SharedMimeInfo allows to look up the MIME type associated to a file name
/// or to the contents of a file, using the [Freedesktop.org Shared MIME
/// database specification][xdg-mime].
///
/// Alongside the MIME type, the Shared MIME database contains other ancillary
/// information, like the icon associated to the MIME type; the aliases for
/// a given MIME type; and the various sub-classes of a MIME type.
///
/// [xdg-mime]: https://specifications.freedesktop.org/shared-mime-info-spec/shared-mime-info-spec-latest.html
use std::env;
use std::path::{Path, PathBuf};

extern crate dirs;
#[macro_use]
extern crate nom;

mod alias;
mod glob;
mod icon;
mod magic;
mod parent;

/// Convenience identifier for an unknown MIME type.
pub static UNKNOWN_TYPE: &str = "application/octet-stream";

/// Convenience identifier for the MIME type for an empty file.
pub static EMPTY_TYPE: &str = "application/x-zerosize";

/// Convenience identifier for the MIME type for a plain text file.
pub static TEXT_PLAIN_TYPE: &str = "text/plain";

pub struct SharedMimeInfo {
    aliases: alias::AliasesList,
    parents: parent::ParentsMap,
    icons: Vec<icon::Icon>,
    generic_icons: Vec<icon::Icon>,
    globs: glob::GlobMap,
    magic: Vec<magic::MagicEntry>,
}

impl Default for SharedMimeInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedMimeInfo {
    fn create() -> SharedMimeInfo {
        SharedMimeInfo {
            aliases: alias::AliasesList::new(),
            parents: parent::ParentsMap::new(),
            icons: Vec::new(),
            generic_icons: Vec::new(),
            globs: glob::GlobMap::new(),
            magic: Vec::new(),
        }
    }

    fn load_directory<P: AsRef<Path>>(&mut self, directory: P) {
        let mut mime_path = PathBuf::new();
        mime_path.push(directory);
        mime_path.push("mime");

        let aliases = alias::read_aliases_from_dir(&mime_path);
        self.aliases.add_aliases(aliases);

        let icons = icon::read_icons_from_dir(&mime_path, false);
        self.icons.extend(icons);

        let generic_icons = icon::read_icons_from_dir(&mime_path, true);
        self.generic_icons.extend(generic_icons);

        let subclasses = parent::read_subclasses_from_dir(&mime_path);
        self.parents.add_subclasses(subclasses);

        let globs = glob::read_globs_from_dir(&mime_path);
        self.globs.add_globs(globs);

        let magic_entries = magic::read_magic_from_dir(&mime_path);
        self.magic.extend(magic_entries);
    }

    /// Creates a new SharedMimeInfo database containing all MIME information
    /// under the [XDG base directories][xdg-base-dir].
    ///
    /// [xdg-base-dir]: http://standards.freedesktop.org/basedir-spec/basedir-spec-latest.html
    pub fn new() -> SharedMimeInfo {
        let mut db = SharedMimeInfo::create();

        let data_home = dirs::data_dir().expect("Data directory is unset");
        db.load_directory(data_home);

        let data_dirs = match env::var_os("XDG_DATA_DIRS") {
            Some(v) => env::split_paths(&v).collect(),
            None => vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ],
        };

        for dir in data_dirs {
            db.load_directory(dir)
        }

        db
    }

    /// Load all the MIME information under @directory, and create a new
    /// SharedMimeInfo for it. This method is only really useful for
    /// testing purposes; you should use SharedMimeInfo::new() instead.
    pub fn new_for_directory<P: AsRef<Path>>(directory: P) -> SharedMimeInfo {
        let mut db = SharedMimeInfo::create();

        db.load_directory(directory);

        db
    }

    /// Retrieves the MIME type aliased by a MIME type, if any.
    pub fn unalias_mime_type(&self, mime_type: &str) -> Option<String> {
        self.aliases.unalias_mime_type(mime_type)
    }

    /// Looks up the icons associated to a MIME type.
    ///
    /// The icons can be looked up within the current icon theme.
    pub fn lookup_icon_names(&self, mime_type: &str) -> Vec<String> {
        let mut res = Vec::new();

        if let Some(v) = icon::find_icon(&self.icons, &mime_type) {
            res.push(v);
        };

        res.push(mime_type.replace("/", "-"));

        match icon::find_icon(&self.generic_icons, mime_type) {
            Some(v) => res.push(v),
            None => {
                let split_type = mime_type.split('/').collect::<Vec<&str>>();

                let generic = format!("{}-x-generic", split_type.get(0).unwrap());
                res.push(generic);
            }
        };

        res
    }

    /// Looks up the generic icon associated to a MIME type.
    ///
    /// The icon can be looked up within the current icon theme.
    pub fn lookup_generic_icon_name(&self, mime_type: &str) -> Option<String> {
        let res = match icon::find_icon(&self.generic_icons, mime_type) {
            Some(v) => v,
            None => {
                let split_type = mime_type.split('/').collect::<Vec<&str>>();

                format!("{}-x-generic", split_type.get(0).unwrap())
            }
        };

        Some(res)
    }

    /// Looks up all the parent MIME types associated to @mime_type
    pub fn get_parents(&self, mime_type: &str) -> Option<Vec<String>> {
        let unaliased = match self.aliases.unalias_mime_type(mime_type) {
            Some(v) => v,
            None => return None,
        };

        let mut res = Vec::new();
        res.push(unaliased.clone());

        if let Some(parents) = self.parents.lookup(unaliased) {
            for parent in parents {
                res.push(parent.clone());
            }
        };

        Some(res)
    }

    /// Retrieves the list of matching MIME types for the given file name,
    /// without looking at the data inside the file.
    pub fn get_mime_types_from_file_name(&self, file_name: &str) -> Vec<String> {
        match self.globs.lookup_mime_type_for_file_name(file_name) {
            Some(v) => v,
            None => {
                let mut res = Vec::new();
                res.push(UNKNOWN_TYPE.to_string());
                res
            }
        }
    }

    /// Retrieves the MIME type for the given data, and the priority of the
    /// match. A priority above 80 means a certain match.
    pub fn get_mime_type_for_data(&self, data: &[u8]) -> Option<(String, u32)> {
        magic::lookup_data(&self.magic, data)
    }

    /// Checks whether two MIME types are equal, taking into account
    /// eventual aliases.
    pub fn mime_type_equal(&self, mime_a: &str, mime_b: &str) -> bool {
        let unaliased_a = self
            .unalias_mime_type(mime_a)
            .unwrap_or_else(|| mime_a.to_string());
        let unaliased_b = self
            .unalias_mime_type(mime_b)
            .unwrap_or_else(|| mime_b.to_string());

        unaliased_a == unaliased_b
    }

    /// Checks whether a MIME type is a subclass of another MIME type
    pub fn mime_type_subclass(&self, mime_type: &str, base: &str) -> bool {
        let unaliased_mime = self
            .unalias_mime_type(mime_type)
            .unwrap_or_else(|| mime_type.to_string());
        let unaliased_base = self
            .unalias_mime_type(base)
            .unwrap_or_else(|| base.to_string());

        if unaliased_mime == unaliased_base {
            return true;
        }

        // Handle super-types
        if unaliased_base.ends_with("/*") {
            let chunks = unaliased_base.split('/').collect::<Vec<&str>>();

            if unaliased_mime.starts_with(chunks.get(0).unwrap()) {
                return true;
            }
        }

        // The text/plain and application/octet-stream require some
        // special handling:
        //
        //  - All text/* types are subclasses of text/plain.
        //  - All streamable types (ie, everything except the
        //    inode/* types) are subclasses of application/octet-stream
        //
        // https://specifications.freedesktop.org/shared-mime-info-spec/shared-mime-info-spec-latest.html#subclassing
        if unaliased_base == "text/plain" && unaliased_mime.starts_with("text/") {
            return true;
        }

        if unaliased_base == "application/octet-stream" && !unaliased_mime.starts_with("inode/") {
            return true;
        }

        if let Some(parents) = self.parents.lookup(&unaliased_mime) {
            for parent in parents {
                if self.mime_type_subclass(parent, &unaliased_base) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn load_test_data() -> SharedMimeInfo {
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let dir = PathBuf::from(&format!("{}/test_files", cwd));
        SharedMimeInfo::new_for_directory(dir)
    }

    #[test]
    fn load_from_directory() {
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let dir = PathBuf::from(&format!("{}/test_files", cwd));
        SharedMimeInfo::new_for_directory(dir);
    }

    #[test]
    fn load_system() {
        let _db = SharedMimeInfo::new();
    }

    #[test]
    fn load_default() {
        let _db: SharedMimeInfo = Default::default();
    }

    #[test]
    fn lookup_generic_icons() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.lookup_generic_icon_name("application/json"),
            Some("text-x-script".to_string())
        );
        assert_eq!(
            mime_db.lookup_generic_icon_name("text/plain"),
            Some("text-x-generic".to_string())
        );
    }

    #[test]
    fn unalias() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.unalias_mime_type("application/ics"),
            Some("text/calendar".to_string())
        );
        assert_eq!(mime_db.unalias_mime_type("text/plain"), None);
    }

    #[test]
    fn mime_type_equal() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.mime_type_equal("application/wordperfect", "application/vnd.wordperfect"),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal("application/x-gnome-app-info", "application/x-desktop"),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal("application/x-wordperfect", "application/vnd.wordperfect"),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal("application/x-wordperfect", "audio/x-midi"),
            false
        );
        assert_eq!(mime_db.mime_type_equal("/", "vnd/vnd"), false);
        assert_eq!(
            mime_db.mime_type_equal("application/octet-stream", "text/plain"),
            false
        );
        assert_eq!(mime_db.mime_type_equal("text/plain", "text/*"), false);
    }

    #[test]
    fn mime_type_for_file_name() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.get_mime_types_from_file_name("foo.txt"),
            vec!["text/plain".to_string()]
        );

        assert_eq!(
            mime_db.get_mime_types_from_file_name("bar.gif"),
            vec!["image/gif".to_string()]
        );
    }

    #[test]
    fn mime_type_for_file_data() {
        let mime_db = load_test_data();

        let svg_data = include_bytes!("../test_files/files/rust-logo.svg");
        assert_eq!(
            mime_db.get_mime_type_for_data(svg_data),
            Some(("image/svg+xml".to_string(), 80))
        );

        let png_data = include_bytes!("../test_files/files/rust-logo.png");
        assert_eq!(
            mime_db.get_mime_type_for_data(png_data),
            Some(("image/png".to_string(), 50))
        );
    }

    #[test]
    fn mime_type_subclass() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.mime_type_subclass("application/rtf", "text/plain"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("message/news", "text/plain"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("message/news", "message/*"),
            true
        );
        assert_eq!(mime_db.mime_type_subclass("message/news", "text/*"), true);
        assert_eq!(
            mime_db.mime_type_subclass("message/news", "application/octet-stream"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("application/rtf", "application/octet-stream"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("application/x-gnome-app-info", "text/plain"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("image/x-djvu", "image/vnd.djvu"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("image/vnd.djvu", "image/x-djvu"),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass("image/vnd.djvu", "text/plain"),
            false
        );
        assert_eq!(
            mime_db.mime_type_subclass("image/vnd.djvu", "text/*"),
            false
        );
        assert_eq!(mime_db.mime_type_subclass("text/*", "text/plain"), true);
    }
}
