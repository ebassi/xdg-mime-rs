use std::fs;
use xdg_mime::SharedMimeInfo;

#[test]
fn reload_observes_in_place_writes_and_directory_removal_or_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let user = tmp.path().join("user");
    let system = tmp.path().join("system");
    fs::create_dir_all(system.join("mime")).unwrap();
    fs::write(system.join("mime/globs2"), "50:text/plain:*.old\n").unwrap();
    let mut db = SharedMimeInfo::new_for_directories([&user, &system]);
    assert_eq!(
        db.get_mime_types_from_file_name("file.old"),
        vec![mime::TEXT_PLAIN]
    );
    assert!(!db.reload());
    fs::write(system.join("mime/globs2"), "50:text/plain:*.replacement\n").unwrap();
    assert!(db.reload());
    assert_eq!(
        db.get_mime_types_from_file_name("file.old"),
        vec![mime::APPLICATION_OCTET_STREAM]
    );
    assert_eq!(
        db.get_mime_types_from_file_name("file.replacement"),
        vec![mime::TEXT_PLAIN]
    );
    fs::create_dir_all(user.join("mime")).unwrap();
    fs::write(
        user.join("mime/globs2"),
        "0:text/plain:__NOGLOBS__\n50:text/plain:*.user\n",
    )
    .unwrap();
    assert!(db.reload());
    assert_eq!(
        db.get_mime_types_from_file_name("file.replacement"),
        vec![mime::APPLICATION_OCTET_STREAM]
    );
    fs::remove_dir_all(&user).unwrap();
    assert!(db.reload());
    assert_eq!(
        db.get_mime_types_from_file_name("file.replacement"),
        vec![mime::TEXT_PLAIN]
    );
    fs::remove_dir_all(&system).unwrap();
    assert!(db.reload());
    assert_eq!(
        db.get_mime_types_from_file_name("file.replacement"),
        vec![mime::APPLICATION_OCTET_STREAM]
    );
    assert!(!db.reload());
}
