use std::fs;
use xdg_mime::SharedMimeInfo;

#[test]
fn missing_files_and_empty_regular_files_are_distinct() {
    let tmp = tempfile::tempdir().unwrap();
    let db = SharedMimeInfo::new_for_directory(tmp.path());
    let missing = tmp.path().join("missing");
    assert_eq!(
        db.guess_mime_type().path(&missing).guess().mime_type(),
        &mime::APPLICATION_OCTET_STREAM
    );
    fs::write(&missing, b"").unwrap();
    assert_eq!(
        db.guess_mime_type()
            .path(&missing)
            .guess()
            .mime_type()
            .as_ref(),
        "application/x-zerosize"
    );
    fs::write(&missing, b"plain text\n").unwrap();
    assert_eq!(
        db.guess_mime_type().path(&missing).guess().mime_type(),
        &mime::TEXT_PLAIN
    );
    assert_eq!(
        db.guess_mime_type()
            .path(tmp.path())
            .guess()
            .mime_type()
            .as_ref(),
        "inode/directory"
    );
}
