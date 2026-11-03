#![cfg(any(unix, target_os = "redox"))]
#![doc(html_root_url = "https://docs.rs/xdg_mime/0.4.0")]
#![allow(dead_code)]

//! `xdg_mime` allows to look up the MIME type associated to a file name
//! or to the contents of a file, using the [Freedesktop.org Shared MIME
//! database specification][xdg-mime].
//!
//! Alongside the MIME type, the shared MIME database contains other ancillary
//! information, like the icon associated to the MIME type; the aliases for
//! a given MIME type; and the various sub-classes of a MIME type.
//!
//! [xdg-mime]: https://specifications.freedesktop.org/shared-mime-info-spec/shared-mime-info-spec-latest.html
//!
//! ## Loading the Shared MIME database
//!
//! The [`SharedMimeInfo`](struct.SharedMimeInfo.html) type will automatically
//! load all the instances of shared MIME databases available in the following
//! directories, in this specified order:
//!
//!  - `$XDG_DATA_HOME/mime`
//!    - if `XDG_DATA_HOME` is unset, this corresponds to `$HOME/.local/share/mime`
//!  - `$XDG_DATA_DIRS/mime`
//!    - if `XDG_DATA_DIRS` is unset, this corresponds to `/usr/local/share/mime`
//!      and `/usr/share/mime`
//!
//! For more information on the `XDG_DATA_HOME` and `XDG_DATA_DIRS` environment
//! variables, see the [XDG base directory specification][xdg-basedir].
//!
//! [xdg-basedir]: https://specifications.freedesktop.org/basedir-spec/latest/
//!
//! The MIME data in each directory will be coalesced into a single database.
//!
//! ## Retrieving the MIME type of a file
//!
//! If you want to know the MIME type of a file, you typically have two
//! options at your disposal:
//!
//!  - guess from the file name, using the [`get_mime_types_from_file_name`]
//!    method
//!  - use an appropriately sized chunk of the file contents and
//!    perform "content sniffing", using the [`get_mime_type_for_data`] method
//!
//! The former step does not come with performance penalties, or even requires
//! the file to exist in the first place, but it may return a list of potential
//! matches; the latter can be an arbitrarily expensive operation to perform,
//! but its result is going to be certain. It is recommended to always guess the
//! MIME type from the file name first, and only use content sniffing lazily and,
//! possibly, asynchronously.
//!
//! [`get_mime_types_from_file_name`]: struct.SharedMimeInfo.html#method.get_mime_types_from_file_name
//! [`get_mime_type_for_data`]: struct.SharedMimeInfo.html#method.get_mime_type_for_data
//!
//! ## Guessing the MIME type
//!
//! If you have access to a file name or its contents, it's possible to use
//! the [`guess_mime_type`] method to create a [`GuessBuilder`] instance, and
//! populate it with the file name, its contents, or the full path to the file;
//! then, call the [`guess`] method to guess the MIME type depending on the
//! available information.
//!
//! [`GuessBuilder`]: struct.GuessBuilder.html
//! [`guess_mime_type`]: struct.SharedMimeInfo.html#method.guess_mime_type
//! [`guess`]: struct.GuessBuilder.html#method.guess

use mime::Mime;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

extern crate dirs_next;
extern crate nom;

mod alias;
mod description;
mod glob;
mod icon;
mod magic;
mod parent;

#[derive(Clone, PartialEq)]
struct MimeDirectory {
    path: PathBuf,
    // Watching only the directory misses in-place writes to existing files.
    stamps: Vec<Option<(SystemTime, u64)>>,
}

impl MimeDirectory {
    fn new(path: PathBuf) -> Self {
        let stamps = [
            "",
            "globs2",
            "globs",
            "magic",
            "aliases",
            "subclasses",
            "icons",
            "generic-icons",
            "types",
            "mime.cache",
        ]
        .iter()
        .map(|name| {
            fs::metadata(path.join(name))
                .ok()
                .and_then(|m| m.modified().ok().map(|time| (time, m.len())))
        })
        .collect();
        Self { path, stamps }
    }
}

/// The shared MIME info database.
pub struct SharedMimeInfo {
    aliases: alias::AliasesList,
    parents: parent::ParentsMap,
    icons: Vec<icon::Icon>,
    generic_icons: Vec<icon::Icon>,
    globs: glob::GlobMap,
    magic: Vec<magic::MagicEntry>,
    disabled_magic: HashSet<Mime>,
    mime_dirs: Vec<MimeDirectory>,
    descriptions: Mutex<HashMap<Mime, description::Comments>>,
}

/// A builder type to specify the parameters for guessing a MIME type.
///
/// Each instance of `GuessBuilder` is tied to the lifetime of the
/// [`SharedMimeInfo`] instance that created it.
///
/// The `GuessBuilder` returned by the [`guess_mime_type`] method is
/// empty, and will always return a `mime::APPLICATION_OCTET_STREAM`
/// guess.
///
/// You can use the builder methods to specify the file name, the data,
/// or both, to be used to guess the MIME type:
///
/// ```rust
/// # use std::error::Error;
/// # use std::str::FromStr;
/// # use mime::Mime;
/// #
/// # fn main() -> Result<(), Box<dyn Error>> {
/// # let mime_db = xdg_mime::SharedMimeInfo::new();
/// // let mime_db = ...
/// let mut guess_builder = mime_db.guess_mime_type();
/// let guess = guess_builder.file_name("foo.png").guess();
/// assert_eq!(guess.mime_type(), &Mime::from_str("image/png")?);
/// #
/// # Ok(())
/// # }
/// ```
///
/// The guessed MIME type can have a degree of uncertainty; for instance,
/// if you only set the [`file_name`] there can be multiple matching MIME
/// types to choose from. Alternatively, if you only set the [`data`], the
/// content might not match any existing rule. Even in the case of setting
/// both the file name and the data the match can be uncertain. This
/// information is preserved by the [`Guess`] type, and can be retrieved
/// using the [`uncertain`] method.
///
/// [`SharedMimeInfo`]: struct.SharedMimeInfo.html
/// [`guess_mime_type`]: struct.SharedMimeInfo.html#method.guess_mime_type
/// [`file_name`]: #method.file_name
/// [`data`]: #method.data
/// [`Guess`]: struct.Guess.html
/// [`uncertain`]: struct.Guess.html#method.uncertain
pub struct GuessBuilder<'a> {
    db: &'a SharedMimeInfo,
    file_name: Option<String>,
    data: Option<Vec<u8>>,
    metadata: Option<fs::Metadata>,
    path: Option<PathBuf>,
    zero_size: bool,
}

/// The result of the [`guess`] method of [`GuessBuilder`].
///
/// [`guess`]: struct.GuessBuilder.html#method.guess
/// [`GuessBuilder`]: struct.GuessBuilder.html
pub struct Guess {
    mime: mime::Mime,
    uncertain: bool,
}

impl<'a> GuessBuilder<'a> {
    /// Sets the file name to be used to guess its MIME type.
    ///
    /// If you have a full path, you should extract the last component,
    /// for instance using the [`Path::file_name()`][path_file_name]
    /// method.
    ///
    /// [path_file_name]: https://doc.rust-lang.org/std/path/struct.Path.html#method.file_name
    pub fn file_name(&mut self, name: &str) -> &mut Self {
        self.file_name = Some(name.to_string());

        self
    }

    /// Sets the data for which you want to guess the MIME type.
    pub fn data(&mut self, data: &[u8]) -> &mut Self {
        // Retain enough bytes for the plain-text fallback even in an empty DB.
        let max_data_size = magic::max_extents(&self.db.magic).max(128);
        self.data = Some(data[..data.len().min(max_data_size)].to_vec());

        self
    }

    /// Sets the metadata of the file for which you want to get the MIME type.
    ///
    /// The metadata can be used to match an existing file or path, for instance:
    ///
    /// ```rust
    /// # use std::error::Error;
    /// use std::fs;
    /// use std::str::FromStr;
    /// use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// # let metadata = fs::metadata("src/lib.rs")?;
    /// // let metadata = fs::metadata("/path/to/lib.rs")?;
    /// let mut guess_builder = mime_db.guess_mime_type();
    /// let guess = guess_builder
    ///     .file_name("lib.rs")
    ///     .metadata(metadata)
    ///     .guess();
    /// assert_eq!(guess.mime_type(), &Mime::from_str("text/rust")?);
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn metadata(&mut self, metadata: fs::Metadata) -> &mut Self {
        self.metadata = Some(metadata);

        self
    }

    /// Sets the path of the file for which you want to get the MIME type.
    ///
    /// The `path` will be used by the [`guess`] method to extract the
    /// file name, metadata, and contents, unless you called the [`file_name`],
    /// [`metadata`], and [`data`] methods, respectively.
    ///
    /// ```rust
    /// # use std::error::Error;
    /// use std::fs;
    /// use std::str::FromStr;
    /// use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// let mut guess_builder = mime_db.guess_mime_type();
    /// let guess = guess_builder
    ///     .path("src")
    ///     .guess();
    /// assert_eq!(guess.mime_type(), &Mime::from_str("inode/directory")?);
    /// #
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`guess`]: #method.guess
    /// [`file_name`]: #method.file_name
    /// [`metadata`]: #method.metadata
    /// [`data`]: #method.data
    pub fn path<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        let mut buf = PathBuf::new();
        buf.push(path);

        self.path = Some(buf);

        self
    }

    /// Sets whether or not the [`guess`] method will return `application/x-zerosize` for empty files.
    ///
    /// If `allow` is set to `true`, using [`guess`] to guess the MIME types of empty files will result in `application/x-zerosize`.
    /// Otherwise, the check for empty files will be disabled and other methods will be used to determine the MIME type.
    /// Does not affect the results of non-empty files.
    ///
    /// Defaults to true.
    ///
    /// [`guess`]: #method.guess
    pub fn zero_size(&mut self, allow: bool) -> &mut Self {
        self.zero_size = allow;
        self
    }

    /// Guesses the MIME type using the data set on the builder. The result is
    /// a [`Guess`] instance that contains both the guessed MIME type, and whether
    /// the result of the guess is certain.
    ///
    /// [`Guess`]: struct.Guess.html
    pub fn guess(&mut self) -> Guess {
        if let Some(path) = &self.path {
            // Fill out the metadata
            if self.metadata.is_none() {
                self.metadata = match fs::metadata(path) {
                    Ok(m) => Some(m),
                    Err(_) => None,
                };
            }

            fn load_data_chunk<P: AsRef<Path>>(path: P, chunk_size: usize) -> Option<Vec<u8>> {
                if chunk_size == 0 {
                    return None;
                }

                let f = match File::open(&path) {
                    Ok(file) => file,
                    Err(_) => return None,
                };

                let mut buf = Vec::new();

                if f.take(chunk_size as u64).read_to_end(&mut buf).is_err() {
                    return None;
                }

                Some(buf)
            }

            // Never open directories or special files (in particular FIFOs).
            if self.data.is_none() {
                if let Some(metadata) = &self.metadata {
                    if metadata.is_file() {
                        let max_data_size = magic::max_extents(&self.db.magic).max(128);
                        self.data = load_data_chunk(path, max_data_size);
                    }
                }
            }

            // Set the file name
            if self.file_name.is_none() {
                if let Some(file_name) = path.file_name() {
                    self.file_name = match file_name.to_os_string().into_string() {
                        Ok(v) => Some(v),
                        Err(_) => None,
                    };
                }
            }
        }

        if let Some(metadata) = &self.metadata {
            let file_type = metadata.file_type();

            // Special type for directories
            if file_type.is_dir() {
                return Guess {
                    mime: "inode/directory".parse::<mime::Mime>().unwrap(),
                    uncertain: true,
                };
            }

            // Special type for symbolic links
            if file_type.is_symlink() {
                return Guess {
                    mime: "inode/symlink".parse::<mime::Mime>().unwrap(),
                    uncertain: true,
                };
            }

            // Special type for empty files
            if self.zero_size && metadata.is_file() && metadata.len() == 0 {
                return Guess {
                    mime: "application/x-zerosize".parse::<mime::Mime>().unwrap(),
                    uncertain: true,
                };
            }
        }

        let name_mime_types: Vec<mime::Mime> = match &self.file_name {
            Some(file_name) => self.db.get_mime_types_from_file_name(file_name),
            None => Vec::new(),
        };

        // File name match, and no conflicts
        if name_mime_types.len() == 1 && name_mime_types[0] != mime::APPLICATION_OCTET_STREAM {
            return Guess {
                mime: name_mime_types[0].clone(),
                uncertain: false,
            };
        }

        let sniffed_mime = match self.data.as_deref() {
            Some([]) if !self.zero_size => (mime::TEXT_PLAIN, 0),
            Some(data) => self.db.get_mime_type_for_data(data).unwrap_or_else(|| {
                if looks_like_text(data) {
                    (mime::TEXT_PLAIN, 0)
                } else {
                    (mime::APPLICATION_OCTET_STREAM, 0)
                }
            }),
            None => (mime::APPLICATION_OCTET_STREAM, 0),
        };

        if name_mime_types.is_empty() {
            // No names and no data => unknown MIME type
            if self.data.is_none() {
                return Guess {
                    mime: mime::APPLICATION_OCTET_STREAM,
                    uncertain: true,
                };
            }

            return Guess {
                mime: sniffed_mime.0.clone(),
                uncertain: sniffed_mime.0 == mime::APPLICATION_OCTET_STREAM,
            };
        } else {
            let (mut mime, priority) = sniffed_mime;

            // From the content type guessing implementation in GIO:
            //
            // For security reasons we don't ever want to sniff desktop files
            // where we know the filename and it doesn't have a .desktop extension.
            // This is because desktop files allow executing any application and
            // we don't want to make it possible to hide them looking like something
            // else.
            if self.file_name.is_some() {
                let x_desktop = "application/x-desktop".parse::<mime::Mime>().unwrap();

                if mime == x_desktop {
                    mime = mime::TEXT_PLAIN;
                }
            }

            if mime != mime::APPLICATION_OCTET_STREAM {
                // We found a match with a high confidence value
                if priority >= 80 {
                    return Guess {
                        mime,
                        uncertain: false,
                    };
                }

                // We have possible conflicts, but the data matches the
                // file name, so let's see if the sniffed MIME type is
                // a subclass of the MIME type associated to the file name,
                // and use that as a tie breaker.
                if name_mime_types
                    .iter()
                    .any(|m| self.db.mime_type_subclass(&mime, m))
                {
                    return Guess {
                        mime,
                        uncertain: false,
                    };
                }
            }

            // If there are conflicts, and the data does not help us,
            // we just pick the first result
            if let Some(mime_type) = name_mime_types.first() {
                return Guess {
                    mime: mime_type.clone(),
                    uncertain: true,
                };
            }
        }

        // Okay, we give up
        Guess {
            mime: mime::APPLICATION_OCTET_STREAM,
            uncertain: true,
        }
    }
}

fn looks_like_text(data: &[u8]) -> bool {
    // "Checking the first 128 bytes of the file for ASCII
    // control characters is a good way to guess whether a
    // file is binary or text."
    // -- shared-mime-info, "Recommended checking order"
    !data
        .iter()
        .take(128)
        .any(|ch| ch.is_ascii_control() && !ch.is_ascii_whitespace())
}

impl Guess {
    /// The guessed MIME type.
    pub fn mime_type(&self) -> &mime::Mime {
        &self.mime
    }

    /// Whether the guessed MIME type is uncertain.
    ///
    /// If the MIME type was guessed only from its file name there can be
    /// multiple matches, but the [`mime_type`] method will return just the
    /// first match.
    ///
    /// If you only have a file name, and you want to gather all potential
    /// matches, you should use the [`get_mime_types_from_file_name`] method
    /// instead of performing a guess.
    ///
    /// [`mime_type`]: #method.mime_type
    /// [`get_mime_types_from_file_name`]: struct.SharedMimeInfo.html#method.get_mime_types_from_file_name
    pub fn uncertain(&self) -> bool {
        self.uncertain
    }
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
            disabled_magic: HashSet::new(),
            mime_dirs: Vec::new(),
            descriptions: Mutex::new(HashMap::new()),
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
        self.globs.add_globs(&globs);

        let magic_entries = magic::read_magic_from_dir(&mime_path);
        magic::add_entries(&mut self.magic, &mut self.disabled_magic, magic_entries);

        self.mime_dirs.push(MimeDirectory::new(mime_path));
    }

    /// Creates a new `SharedMimeInfo` instance containing all MIME information
    /// under the [standard XDG base directories][xdg-basedir].
    ///
    /// [xdg-basedir]: http://standards.freedesktop.org/basedir-spec/basedir-spec-latest.html
    pub fn new() -> SharedMimeInfo {
        let mut db = SharedMimeInfo::create();

        if let Some(data_home) = dirs_next::data_dir() {
            db.load_directory(data_home);
        }

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

    /// Loads all the MIME information under `directory`, and creates a new
    /// [`SharedMimeInfo`] instance for it.
    ///
    /// This method is only really useful for testing purposes; you should
    /// always use the [`new`] method, instead.
    ///
    /// [`SharedMimeInfo`]: struct.SharedMimeInfo.html
    /// [`new`]: #method.new
    pub fn new_for_directory<P: AsRef<Path>>(directory: P) -> SharedMimeInfo {
        Self::new_for_directories([directory])
    }

    /// Loads MIME databases from the supplied XDG data directories, in
    /// descending precedence (user directories before system directories).
    /// Each directory must contain a `mime` subdirectory.
    ///
    /// This allows applications and tests to use isolated databases without
    /// changing process-wide environment variables.
    pub fn new_for_directories<I, P>(directories: I) -> SharedMimeInfo
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut db = SharedMimeInfo::create();
        for directory in directories {
            db.load_directory(directory);
        }
        db
    }

    /// Reloads the contents of the [`SharedMimeInfo`] type from the directories
    /// used to populate it at construction time. You should use this method
    /// if you're planning to keep the database around for long running operations
    /// or applications.
    ///
    /// This method does not do anything if the directories haven't changed
    /// since the time they were loaded last.
    ///
    /// This method will return `true` if the contents of the shared MIME
    /// database were updated.
    ///
    /// [`SharedMimeInfo`]: struct.SharedMimeInfo.html
    pub fn reload(&mut self) -> bool {
        let changed = self
            .mime_dirs
            .iter()
            .any(|dir| *dir != MimeDirectory::new(dir.path.clone()));
        if changed {
            let dirs: Vec<_> = self
                .mime_dirs
                .iter()
                .filter_map(|dir| dir.path.parent().map(Path::to_path_buf))
                .collect();
            *self = Self::new_for_directories(dirs);
        }
        changed
    }

    /// Retrieves the MIME type aliased by a MIME type, if any.
    pub fn unalias_mime_type(&self, mime_type: &Mime) -> Option<Mime> {
        self.aliases.unalias_mime_type(mime_type)
    }

    /// Returns the human-readable comment for a MIME type, following aliases.
    ///
    /// `languages` lists preferred locales in order, for example `["fr_CA", "en"]`.
    /// Territory, encoding and modifier fallbacks are supported. If no translation
    /// matches, the untagged comment is returned; `C` and `POSIX` request that
    /// fallback immediately. Returns `None` when no description is installed.
    ///
    /// Generated per-type XML files are loaded lazily and cached until [`reload`].
    /// Higher-precedence directories override comments for the same language.
    ///
    /// [`reload`]: Self::reload
    pub fn lookup_description(&self, mime_type: &Mime, languages: &[&str]) -> Option<String> {
        let canonical = self
            .unalias_mime_type(mime_type)
            .unwrap_or_else(|| mime_type.clone());
        let mut descriptions = self.descriptions.lock().unwrap_or_else(|e| e.into_inner());
        let comments = descriptions.entry(canonical.clone()).or_insert_with(|| {
            description::load(
                self.mime_dirs.iter().map(|dir| dir.path.clone()),
                &canonical,
            )
        });
        description::lookup(comments, languages)
    }

    /// Looks up the icons associated to a MIME type.
    ///
    /// The icons can be looked up within the current [icon theme][xdg-icon-theme].
    ///
    /// [xdg-icon-theme]: https://specifications.freedesktop.org/icon-theme-spec/icon-theme-spec-latest.html
    pub fn lookup_icon_names(&self, mime_type: &Mime) -> Vec<String> {
        let mut res = Vec::new();

        if let Some(v) = icon::find_icon(&self.icons, mime_type) {
            res.push(v);
        };

        res.push(mime_type.essence_str().replace('/', "-"));

        match icon::find_icon(&self.generic_icons, mime_type) {
            Some(v) => res.push(v),
            None => {
                let generic = format!("{}-x-generic", mime_type.type_());
                res.push(generic);
            }
        };

        res
    }

    /// Looks up the generic icon associated to a MIME type.
    ///
    /// The icon can be looked up within the current [icon theme][xdg-icon-theme].
    ///
    /// [xdg-icon-theme]: https://specifications.freedesktop.org/icon-theme-spec/icon-theme-spec-latest.html
    pub fn lookup_generic_icon_name(&self, mime_type: &Mime) -> Option<String> {
        let res = match icon::find_icon(&self.generic_icons, mime_type) {
            Some(v) => v,
            None => format!("{}-x-generic", mime_type.type_()),
        };

        Some(res)
    }

    /// Retrieves all the parent MIME types associated to `mime_type`.
    pub fn get_parents(&self, mime_type: &Mime) -> Option<Vec<Mime>> {
        let unaliased = match self.aliases.unalias_mime_type(mime_type) {
            Some(v) => v,
            None => return None,
        };

        let mut res = vec![unaliased.clone()];

        if let Some(parents) = self.parents.lookup(&unaliased) {
            for parent in parents {
                res.push(parent.clone());
            }
        };

        Some(res)
    }

    /// Retrieves all the parent MIME types associated to `mime_type`, without unaliasing.
    /// You may want to use this to get a list of sub-classes, eg:
    ///
    /// ```rust
    /// use mime::Mime;
    /// use std::str::FromStr;
    /// use xdg_mime::SharedMimeInfo;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///
    /// let parents = (SharedMimeInfo::new()).get_parents_aliased(&Mime::from_str("application/toml")?).unwrap_or_default();
    /// # let _ = parents;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_parents_aliased(&self, mime_type: &Mime) -> Option<Vec<Mime>> {
        let mut res = Vec::new();

        if let Some(parents) = self.parents.lookup(&mime_type) {
            for parent in parents {
                res.push(parent.clone());
            }
        };

        match res.len() {
            0 => None,
            _ => Some(res),
        }
    }

    /// Retrieves the list of matching MIME types for the given file name,
    /// without looking at the data inside the file.
    ///
    /// If no specific MIME-type can be determined, returns a single
    /// element vector containing the `application/octet-stream` MIME type.
    ///
    /// ```rust
    /// # use std::error::Error;
    /// # use std::str::FromStr;
    /// # use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// let mime_types: Vec<Mime> = mime_db.get_mime_types_from_file_name("file.txt");
    /// assert_eq!(mime_types, vec![Mime::from_str("text/plain")?]);
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_mime_types_from_file_name(&self, file_name: &str) -> Vec<Mime> {
        match self.globs.lookup_mime_type_for_file_name(file_name) {
            Some(v) => v,
            None => {
                vec![mime::APPLICATION_OCTET_STREAM.clone()]
            }
        }
    }

    /// Retrieves the MIME type for the given data, and the priority of the
    /// match. A priority above 80 means a certain match.
    pub fn get_mime_type_for_data(&self, data: &[u8]) -> Option<(Mime, u32)> {
        if data.is_empty() {
            let empty_mime: mime::Mime = "application/x-zerosize".parse().unwrap();
            return Some((empty_mime, 100));
        }

        magic::lookup_data(&self.magic, data)
    }

    /// Checks whether two MIME types are equal, taking into account
    /// eventual aliases.
    ///
    /// ```rust
    /// # use std::error::Error;
    /// # use std::str::FromStr;
    /// # use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// let x_markdown: Mime = "text/x-markdown".parse()?;
    /// let markdown: Mime = "text/markdown".parse()?;
    /// assert!(mime_db.mime_type_equal(&x_markdown, &markdown));
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn mime_type_equal(&self, mime_a: &Mime, mime_b: &Mime) -> bool {
        let unaliased_a = self
            .unalias_mime_type(mime_a)
            .unwrap_or_else(|| mime_a.clone());
        let unaliased_b = self
            .unalias_mime_type(mime_b)
            .unwrap_or_else(|| mime_b.clone());

        unaliased_a == unaliased_b
    }

    /// Checks whether a MIME type is a subclass of another MIME type.
    ///
    /// ```rust
    /// # use std::error::Error;
    /// # use std::str::FromStr;
    /// # use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// let rust: Mime = "text/rust".parse()?;
    /// let text: Mime = "text/plain".parse()?;
    /// assert!(mime_db.mime_type_subclass(&rust, &text));
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn mime_type_subclass(&self, mime_type: &Mime, base: &Mime) -> bool {
        let unaliased_base = self.unalias_mime_type(base).unwrap_or_else(|| base.clone());
        let mut pending = vec![mime_type.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            let unaliased_mime = self.unalias_mime_type(&current).unwrap_or(current);
            if !visited.insert(unaliased_mime.clone()) {
                continue;
            }
            if unaliased_mime == unaliased_base {
                return true;
            }

            // Handle super-types
            if unaliased_base.subtype() == mime::STAR {
                let base_type = unaliased_base.type_();
                let unaliased_type = unaliased_mime.type_();

                if base_type == unaliased_type {
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
            if unaliased_base == mime::TEXT_PLAIN && unaliased_mime.type_() == mime::TEXT {
                return true;
            }

            if unaliased_base == mime::APPLICATION_OCTET_STREAM && unaliased_mime.type_() != "inode"
            {
                return true;
            }

            if let Some(parents) = self.parents.lookup(&unaliased_mime) {
                pending.extend(parents.iter().cloned());
            }
        }

        false
    }

    /// Creates a new [`GuessBuilder`] that can be used to guess the MIME type
    /// of a file name, its contents, or a path.
    ///
    /// ```rust
    /// # use std::error::Error;
    /// # use std::str::FromStr;
    /// # use mime::Mime;
    /// #
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let mime_db = xdg_mime::SharedMimeInfo::new();
    /// // let mime_db = ...
    /// let mut gb = mime_db.guess_mime_type();
    /// let guess = gb.file_name("foo.txt").guess();
    /// assert_eq!(guess.mime_type(), &mime::TEXT_PLAIN);
    /// assert_eq!(guess.uncertain(), false);
    /// #
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`GuessBuilder`]: struct.GuessBuilder.html
    pub fn guess_mime_type(&self) -> GuessBuilder<'_> {
        GuessBuilder {
            db: self,
            file_name: None,
            data: None,
            metadata: None,
            path: None,
            zero_size: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::str::FromStr;

    #[test]
    fn absent_data_empty_data_and_plain_text_are_distinct() {
        let db = SharedMimeInfo::create();
        assert_eq!(
            db.guess_mime_type()
                .file_name("unknown")
                .guess()
                .mime_type(),
            &mime::APPLICATION_OCTET_STREAM
        );
        assert_eq!(
            db.guess_mime_type().data(b"").guess().mime_type().as_ref(),
            "application/x-zerosize"
        );
        assert_eq!(
            db.guess_mime_type().data(b"hello\n").guess().mime_type(),
            &mime::TEXT_PLAIN
        );
        assert_eq!(
            db.guess_mime_type().data(b"\0binary").guess().mime_type(),
            &mime::APPLICATION_OCTET_STREAM
        );
        let mut guess = db.guess_mime_type();
        guess.data(b"\0binary").data(b"hello\n");
        assert_eq!(guess.guess().mime_type(), &mime::TEXT_PLAIN);
    }

    #[test]
    fn inheritance_cycles_terminate_and_preserve_other_parents() {
        let mut db = SharedMimeInfo::create();
        let a: Mime = "application/x-a".parse().unwrap();
        let b: Mime = "application/x-b".parse().unwrap();
        db.parents.add_subclasses(vec![
            parent::Subclass::new(&a, &b),
            parent::Subclass::new(&b, &a),
            parent::Subclass::new(&b, &mime::TEXT_PLAIN),
        ]);
        assert!(db.mime_type_subclass(&a, &mime::TEXT_PLAIN));
        assert!(!db.mime_type_subclass(&a, &mime::IMAGE_PNG));
    }

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
    fn reload() {
        // We don't load the system data in the, admittedly, remote case the system
        // is getting updated *while* we run the test suite.
        let mut _db = load_test_data();

        assert_eq!(_db.reload(), false);
    }

    #[test]
    fn lookup_generic_icons() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.lookup_generic_icon_name(&mime::APPLICATION_JSON),
            Some("text-x-script".to_string())
        );
        assert_eq!(
            mime_db.lookup_generic_icon_name(&mime::TEXT_PLAIN),
            Some("text-x-generic".to_string())
        );
    }

    #[test]
    fn unalias() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.unalias_mime_type(&Mime::from_str("application/ics").unwrap()),
            Some(Mime::from_str("text/calendar").unwrap())
        );
        assert_eq!(
            mime_db.unalias_mime_type(&Mime::from_str("text/plain").unwrap()),
            None
        );
    }

    #[test]
    fn mime_type_equal() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("application/wordperfect").unwrap(),
                &Mime::from_str("application/vnd.wordperfect").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("application/x-gnome-app-info").unwrap(),
                &Mime::from_str("application/x-desktop").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("application/x-wordperfect").unwrap(),
                &Mime::from_str("application/vnd.wordperfect").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("application/x-wordperfect").unwrap(),
                &Mime::from_str("audio/x-midi").unwrap(),
            ),
            false
        );
        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("application/octet-stream").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            false
        );
        assert_eq!(
            mime_db.mime_type_equal(
                &Mime::from_str("text/plain").unwrap(),
                &Mime::from_str("text/*").unwrap(),
            ),
            false
        );
    }

    #[test]
    fn mime_type_for_file_name() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.get_mime_types_from_file_name("foo.txt"),
            vec![Mime::from_str("text/plain").unwrap()]
        );

        assert_eq!(
            mime_db.get_mime_types_from_file_name("bar.gif"),
            vec![Mime::from_str("image/gif").unwrap()]
        );

        assert_eq!(
            mime_db.get_mime_types_from_file_name("baz.mod"),
            vec![Mime::from_str("audio/x-mod").unwrap()]
        );
    }

    #[test]
    fn mime_type_for_file_data() {
        let mime_db = load_test_data();

        let svg_data = include_bytes!("../test_files/files/rust-logo.svg");
        assert_eq!(
            mime_db.get_mime_type_for_data(svg_data),
            Some((Mime::from_str("image/svg+xml").unwrap(), 80))
        );

        let png_data = include_bytes!("../test_files/files/rust-logo.png");
        assert_eq!(
            mime_db.get_mime_type_for_data(png_data),
            Some((Mime::from_str("image/png").unwrap(), 50))
        );
    }

    #[test]
    fn mime_type_subclass() {
        let mime_db = load_test_data();

        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("application/rtf").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("message/news").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("message/news").unwrap(),
                &Mime::from_str("message/*").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("message/news").unwrap(),
                &Mime::from_str("text/*").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("message/news").unwrap(),
                &Mime::from_str("application/octet-stream").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("application/rtf").unwrap(),
                &Mime::from_str("application/octet-stream").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("application/x-gnome-app-info").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("image/x-djvu").unwrap(),
                &Mime::from_str("image/vnd.djvu").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("image/vnd.djvu").unwrap(),
                &Mime::from_str("image/x-djvu").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("image/vnd.djvu").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            false
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("image/vnd.djvu").unwrap(),
                &Mime::from_str("text/*").unwrap(),
            ),
            false
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("text/*").unwrap(),
                &Mime::from_str("text/plain").unwrap(),
            ),
            true
        );
        assert_eq!(
            mime_db.mime_type_subclass(
                &Mime::from_str("application/x-shellscript").unwrap(),
                &mime::APPLICATION_OCTET_STREAM
            ),
            true
        );
    }

    #[test]
    fn guess_none() {
        let mime_db = load_test_data();

        let mut gb = mime_db.guess_mime_type();
        let guess = gb.guess();
        assert_eq!(guess.mime_type(), &mime::APPLICATION_OCTET_STREAM);
        assert_eq!(guess.uncertain(), true);
    }

    #[test]
    fn guess_filename() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb.file_name("foo.txt").guess();
        assert_eq!(guess.mime_type(), &mime::TEXT_PLAIN);
        assert_eq!(guess.uncertain(), false);
    }

    #[test]
    fn guess_data() {
        let svg_data = include_bytes!("../test_files/files/rust-logo.svg");
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb.data(svg_data).guess();
        assert_eq!(guess.mime_type(), &Mime::from_str("image/svg+xml").unwrap());
        assert_eq!(guess.uncertain(), false);
    }

    #[test]
    fn guess_both() {
        let png_data = include_bytes!("../test_files/files/rust-logo.png");
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb.file_name("rust-logo.png").data(png_data).guess();
        assert_eq!(guess.mime_type(), &Mime::from_str("image/png").unwrap());
        assert_eq!(guess.uncertain(), false);
    }

    #[test]
    fn guess_script() {
        let sh_data = include_bytes!("../test_files/files/script");
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb.data(sh_data).guess();
        assert_eq!(
            guess.mime_type(),
            &Mime::from_str("application/x-shellscript").unwrap()
        );
    }

    #[test]
    fn guess_script_with_name() {
        let sh_data = include_bytes!("../test_files/files/gp");
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb.file_name("gp").data(sh_data).guess();
        assert_eq!(
            guess.mime_type(),
            &Mime::from_str("application/x-shellscript").unwrap()
        );
    }

    #[test]
    fn guess_empty() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let file = PathBuf::from(&format!("{}/test_files/files/empty", cwd));
        let guess = gb.path(file).guess();
        assert_ne!(guess.mime_type(), &mime::TEXT_PLAIN);
        assert_eq!(
            guess.mime_type(),
            &Mime::from_str("application/x-zerosize").unwrap()
        );
    }

    #[test]
    fn guess_empty_no_zero_size() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let file = PathBuf::from(&format!("{}/test_files/files/empty.json", cwd));
        let guess = gb.path(file.clone()).guess();
        assert_ne!(guess.mime_type(), &mime::APPLICATION_JSON);
        assert_eq!(
            guess.mime_type(),
            &Mime::from_str("application/x-zerosize").unwrap()
        );
        let guess = gb.path(file).zero_size(false).guess();
        assert_ne!(
            guess.mime_type(),
            &Mime::from_str("application/x-zerosize").unwrap()
        );
        assert_eq!(guess.mime_type(), &mime::APPLICATION_JSON);
    }

    #[test]
    fn guess_empty_no_zero_size_no_extension() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let file = PathBuf::from(&format!("{}/test_files/files/empty", cwd));
        let guess = gb.path(file.clone()).guess();
        assert_eq!(
            guess.mime_type(),
            &Mime::from_str("application/x-zerosize").unwrap()
        );
        let guess = gb.path(file).zero_size(false).guess();
        assert_eq!(guess.mime_type(), &mime::TEXT_PLAIN);
    }

    #[test]
    fn guess_text() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let file = PathBuf::from(&format!("{}/test_files/files/text", cwd));
        let guess = gb.path(file).guess();
        assert_eq!(guess.mime_type(), &mime::TEXT_PLAIN);
    }

    #[test]
    fn looks_like_text_works() {
        assert!(looks_like_text(&[]));
        assert!(looks_like_text(b"hello"));
        assert!(!looks_like_text(b"hello\x00"));
        assert!(!looks_like_text(&[0, 1, 2]));
    }

    #[test]
    fn guess_turtle() {
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let ttl_file = PathBuf::from(&format!("{}/test_files/files/example.ttl", cwd));
        let ttl_data = include_bytes!("../test_files/files/example.ttl");
        let ttl_meta = std::fs::metadata(ttl_file).unwrap();
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb
            .file_name("example.ttl")
            .metadata(ttl_meta)
            .data(ttl_data)
            .guess();
        assert_eq!(guess.mime_type(), &Mime::from_str("text/turtle").unwrap());
    }

    #[test]
    fn guess_dodgy_desktop_file() {
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let desktop_file = PathBuf::from(&format!("{}/test_files/files/launcher", cwd));
        let desktop_data = include_bytes!("../test_files/files/launcher");
        let desktop_meta = std::fs::metadata(desktop_file).unwrap();
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let guess = gb
            .file_name("launcher")
            .metadata(desktop_meta)
            .data(desktop_data)
            .guess();
        assert_eq!(guess.mime_type(), &Mime::from_str("text/plain").unwrap());
    }

    #[test]
    fn guess_html_with_no_html_tags() {
        let mime_db = load_test_data();
        let mut gb = mime_db.guess_mime_type();
        let cwd = env::current_dir().unwrap().to_string_lossy().into_owned();
        let file = PathBuf::from(&format!("{}/test_files/files/no_html_tags.html", cwd));
        let guess = gb.path(file).guess();
        assert_eq!(guess.mime_type(), &mime::TEXT_HTML);
    }
}
