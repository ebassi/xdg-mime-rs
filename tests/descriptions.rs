use std::fs;
use xdg_mime::SharedMimeInfo;

#[test]
fn translated_comments_aliases_precedence_malformed_xml_and_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let user = tmp.path().join("user");
    let system = tmp.path().join("system");
    for dir in [&user, &system] {
        fs::create_dir_all(dir.join("mime/text")).unwrap();
    }
    let xml = |comments: &str| {
        format!("<mime-type xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\" type=\"text/plain\">{comments}</mime-type>")
    };
    fs::write(system.join("mime/text/plain.xml"), xml("<comment>Text &amp; notes</comment><comment xml:lang=\"fr\">Texte</comment><comment xml:lang=\"ja\">テキスト</comment>")).unwrap();
    fs::write(
        user.join("mime/text/plain.xml"),
        xml("<comment xml:lang=\"fr\">Document</comment>"),
    )
    .unwrap();
    fs::write(system.join("mime/aliases"), "text/x-test text/plain\n").unwrap();
    let mut db = SharedMimeInfo::new_for_directories([&user, &system]);
    let alias = "text/x-test".parse().unwrap();
    assert_eq!(
        db.lookup_description(&alias, &["fr_CA.UTF-8"]),
        Some("Document".into())
    );
    assert_eq!(
        db.lookup_description(&alias, &["zz", "ja_JP"]),
        Some("テキスト".into())
    );
    assert_eq!(
        db.lookup_description(&alias, &["C", "fr"]),
        Some("Text & notes".into())
    );
    assert_eq!(
        db.lookup_description(&alias, &[]),
        Some("Text & notes".into())
    );
    assert_eq!(db.lookup_description(&mime::IMAGE_PNG, &[]), None);
    fs::write(user.join("mime/text/plain.xml"), "<malformed>").unwrap();
    // update-mime-database also rewrites the index files.
    fs::write(user.join("mime/types"), "text/plain\n").unwrap();
    assert!(db.reload());
    assert_eq!(db.lookup_description(&alias, &["fr"]), Some("Texte".into()));
}
