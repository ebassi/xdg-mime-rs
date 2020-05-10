#![cfg(any(unix, target_os = "redox"))]
// FIXME: Remove
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

impl SharedMimeInfo {
    fn load_directory<P: AsRef<Path>>(&mut self, directory: P) {
        let mut mime_path = PathBuf::new();
        mime_path.push(directory);
        mime_path.push("mime");

        let mut alias_file = mime_path.clone();
        alias_file.push("aliases");
        let aliases = alias::read_aliases_from_file(alias_file);
        self.aliases.add_aliases(aliases);

        let mut icons_file = mime_path.clone();
        icons_file.push("icons");
        let icons = icon::read_icons_from_file(icons_file);
        self.icons.extend(icons);

        icons_file = mime_path.clone();
        icons_file.push("generic-icons");
        let generic_icons = icon::read_icons_from_file(icons_file);
        self.generic_icons.extend(generic_icons);

        let mut subclasses_file = mime_path.clone();
        subclasses_file.push("subclasses");
        let subclasses = parent::read_subclasses_from_file(subclasses_file);
        self.parents.add_subclasses(subclasses);

        let mut glob_v2_file = mime_path.clone();
        glob_v2_file.push("globs2");
        let globs = match glob::read_globs_v2_from_file(glob_v2_file) {
            Some(v) => v,
            None => {
                let mut glob_v1_file = mime_path.clone();
                glob_v1_file.push("globs");

                glob::read_globs_v1_from_file(glob_v1_file).unwrap_or(Vec::new())
            }
        };

        self.globs.add_globs(globs);

        let mut magic_file = mime_path.clone();
        magic_file.push("magic");
        let magic_entries = magic::read_magic_from_file(magic_file);
        self.magic.extend(magic_entries);
    }

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
    /// testing purposes.
    pub fn new_for_directory<P: AsRef<Path>>(directory: P) -> SharedMimeInfo {
        let mut mime_path = PathBuf::new();
        mime_path.push(directory);
        mime_path.push("mime");

        let mut alias_file = mime_path.clone();
        alias_file.push("aliases");
        let mut alias_list = alias::AliasesList::new();
        let aliases = alias::read_aliases_from_file(alias_file);
        alias_list.add_aliases(aliases);

        let mut icons_file = mime_path.clone();
        icons_file.push("icons");
        let icons = icon::read_icons_from_file(icons_file);

        icons_file = mime_path.clone();
        icons_file.push("generic-icons");
        let generic_icons = icon::read_icons_from_file(icons_file);

        let mut subclasses_file = mime_path.clone();
        subclasses_file.push("subclasses");
        let mut parents_map = parent::ParentsMap::new();
        let subclasses = parent::read_subclasses_from_file(subclasses_file);
        parents_map.add_subclasses(subclasses);

        let mut glob_v2_file = mime_path.clone();
        glob_v2_file.push("globs2");
        let globs = match glob::read_globs_v2_from_file(glob_v2_file) {
            Some(v) => v,
            None => {
                let mut glob_v1_file = mime_path.clone();
                glob_v1_file.push("globs");

                glob::read_globs_v1_from_file(glob_v1_file).unwrap_or(Vec::new())
            }
        };

        let mut glob_map = glob::GlobMap::new();
        glob_map.add_globs(globs);

        let mut magic_file = mime_path.clone();
        magic_file.push("magic");
        let magic_entries = magic::read_magic_from_file(magic_file);

        SharedMimeInfo {
            aliases: alias_list,
            parents: parents_map,
            globs: glob_map,
            icons,
            generic_icons,
            magic: magic_entries,
        }
    }

    /// Retrieves the MIME type aliased by @mime_type, if any.
    pub fn unalias_mime_type(&self, mime_type: &str) -> Option<String> {
        self.aliases.unalias_mime_type(mime_type)
    }

    /// Looks up the icons associated to a MIME type.
    ///
    /// The icons can be looked up within the current icon theme.
    pub fn lookup_icon_names(&self, mime_type: &str) -> Vec<String> {
        let mut res = Vec::new();

        match icon::find_icon(&self.icons, &mime_type) {
            Some(v) => res.push(v),
            None => {}
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

        match self.parents.lookup(unaliased) {
            Some(v) => {
                for parent in v {
                    res.push(parent.clone());
                }
            }
            None => {}
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

    /// Retrieves the MIME type for the given data.
    pub fn get_mime_type_for_data(&self, data: &[u8]) -> Option<String> {
        let mime_type = match magic::lookup_data(&self.magic, data) {
            Some(v) => v.0,
            None => return None,
        };

        Some(mime_type)
    }

    /// Checks whether two MIME types are equal, taking into account
    /// eventual aliases.
    pub fn mime_type_equal(&self, mime_a: &str, mime_b: &str) -> bool {
        let unaliased_a = self.unalias_mime_type(mime_a).unwrap_or(mime_a.to_string());
        let unaliased_b = self.unalias_mime_type(mime_b).unwrap_or(mime_b.to_string());

        unaliased_a == unaliased_b
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
        SharedMimeInfo::new();
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
            Some("image/svg+xml".to_string())
        );

        let png_data = include_bytes!("../test_files/files/rust-logo.png");
        assert_eq!(
            mime_db.get_mime_type_for_data(png_data),
            Some("image/png".to_string())
        );
    }
}
