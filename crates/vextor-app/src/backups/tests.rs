use std::fs;
use std::time::{Duration, UNIX_EPOCH};

use super::{Backups, Settings, age, entries, size};
use crate::files::test_dir::TestDir;

/// An app-data folder and a separate "Documents" folder.
struct Setup {
    app: TestDir,
    docs: TestDir,
}

fn setup() -> Setup {
    Setup {
        app: TestDir::new(),
        docs: TestDir::new(),
    }
}

#[test]
fn overwriting_an_existing_file_backs_it_up_once_per_session() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    fs::write(&file, b"version one").expect("writes");
    let mut backups = Backups::open(s.app.path());

    backups.before_overwrite(&file).expect("backs up");
    fs::write(&file, b"version two").expect("writes");
    backups.before_overwrite(&file).expect("no second backup");

    let list = backups.of(&file);
    assert_eq!(list.len(), 1);
    assert_eq!(fs::read(&list[0].path).expect("reads"), b"version one");
    assert!(list[0].path.starts_with(s.app.path().join("backups")));
    assert!(
        !list[0].path.starts_with(s.docs.path()),
        "never next to the file"
    );
}

#[test]
fn a_new_file_needs_no_backup() {
    let s = setup();
    let mut backups = Backups::open(s.app.path());
    let file = s.docs.path().join("new.vxtr");
    backups.before_overwrite(&file).expect("nothing to do");
    assert_eq!(backups.of(&file).len(), 0);
    assert_eq!(backups.usage(), 0);
}

#[test]
fn the_next_session_backs_up_again() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    fs::write(&file, b"one").expect("writes");
    Backups::open(s.app.path())
        .before_overwrite(&file)
        .expect("backs up");
    fs::write(&file, b"two").expect("writes");
    let mut next = Backups::open(s.app.path());
    next.before_overwrite(&file).expect("backs up");
    let contents: Vec<Vec<u8>> = next
        .of(&file)
        .iter()
        .map(|b| fs::read(&b.path).expect("reads"))
        .collect();
    assert_eq!(contents, [b"two".to_vec(), b"one".to_vec()], "newest first");
}

#[test]
fn a_failed_backup_refuses_the_write() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    fs::write(&file, b"precious").expect("writes");
    // The backups folder can't be created: a file is in its way.
    fs::write(s.app.path().join("backups"), b"in the way").expect("writes");
    let mut backups = Backups::open(s.app.path());
    assert!(backups.before_overwrite(&file).is_err());
    // And it is asked again next time rather than taken as done.
    fs::remove_file(s.app.path().join("backups")).expect("removes");
    backups.before_overwrite(&file).expect("backs up now");
    assert_eq!(backups.of(&file).len(), 1);
}

#[test]
fn backups_off_makes_none() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    fs::write(&file, b"x").expect("writes");
    let mut backups = Backups::open(s.app.path());
    backups
        .set_settings(Settings {
            enabled: false,
            ..Settings::default()
        })
        .expect("saves settings");
    backups.before_overwrite(&file).expect("nothing to do");
    assert_eq!(backups.of(&file).len(), 0);
    assert!(
        !Backups::open(s.app.path()).settings().enabled,
        "remembered"
    );
}

#[test]
fn each_file_keeps_only_its_newest_backups() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    for i in 0..8 {
        fs::write(&file, format!("version {i}")).expect("writes");
        // A new session each time, so each overwrite backs up.
        Backups::open(s.app.path())
            .before_overwrite(&file)
            .expect("backs up");
    }
    let backups = Backups::open(s.app.path());
    let kept: Vec<String> = backups
        .of(&file)
        .iter()
        .map(|b| String::from_utf8(fs::read(&b.path).expect("reads")).expect("utf-8"))
        .collect();
    assert_eq!(kept.len(), Settings::default().per_file);
    assert_eq!(kept[0], "version 7", "the newest kept");
    assert_eq!(kept[4], "version 3");
}

#[test]
fn all_backups_together_stay_under_the_size_limit() {
    let s = setup();
    let mut backups = Backups::open(s.app.path());
    backups
        .set_settings(Settings {
            total_mb: 10,
            ..Settings::default()
        })
        .expect("saves");
    let mb = vec![b'x'; 1024 * 1024];
    for i in 0..14 {
        let file = s.docs.path().join(format!("f{i}.vxtr"));
        fs::write(&file, &mb).expect("writes");
        backups.before_overwrite(&file).expect("backs up");
    }
    assert!(backups.usage() <= 10 * 1024 * 1024, "{}", backups.usage());
    // The oldest went first.
    assert_eq!(backups.of(&s.docs.path().join("f0.vxtr")).len(), 0);
    assert_eq!(backups.of(&s.docs.path().join("f13.vxtr")).len(), 1);
}

#[test]
fn startup_prunes_to_the_settings() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    for i in 0..4 {
        fs::write(&file, format!("v{i}")).expect("writes");
        Backups::open(s.app.path())
            .before_overwrite(&file)
            .expect("backs up");
    }
    let settings = Settings {
        per_file: 2,
        ..Settings::default()
    };
    let dir = s.app.path().join("backups");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_vec(&settings).expect("encodes"),
    )
    .expect("writes");
    assert_eq!(Backups::open(s.app.path()).of(&file).len(), 2);
}

#[test]
fn a_saved_file_needs_no_backup_of_this_session_s_own_work() {
    let s = setup();
    let file = s.docs.path().join("icon.vxtr");
    fs::write(&file, b"saved by us").expect("writes");
    let mut backups = Backups::open(s.app.path());
    backups.written(&file);
    backups.before_overwrite(&file).expect("nothing to do");
    assert_eq!(backups.of(&file).len(), 0);
}

#[test]
fn damaged_settings_fall_back_to_the_defaults() {
    let s = setup();
    let dir = s.app.path().join("backups");
    fs::create_dir_all(&dir).expect("creates");
    fs::write(dir.join("settings.json"), b"{not json").expect("writes");
    assert_eq!(Backups::open(s.app.path()).settings(), Settings::default());
    fs::write(
        dir.join("settings.json"),
        br#"{"per_file": 0, "total_mb": 999999}"#,
    )
    .expect("writes");
    let clamped = Backups::open(s.app.path()).settings();
    assert_eq!(clamped.per_file, 1);
    assert_eq!(clamped.total_mb, 2_000);
}

#[test]
fn stray_files_in_a_backup_folder_are_not_backups() {
    let s = setup();
    let folder = s.app.path().join("backups").join("0123456789abcdef");
    fs::create_dir_all(&folder).expect("creates");
    fs::write(folder.join("index.json"), b"{}").expect("writes");
    fs::write(folder.join(".1-0.partial"), b"half").expect("writes");
    assert_eq!(entries(&folder).len(), 0);
}

#[test]
fn ages_and_sizes_read_simply() {
    let t = UNIX_EPOCH + Duration::from_secs(1_000_000);
    let after = |s| t + Duration::from_secs(s);
    assert_eq!(age(t, after(5)), "just now");
    assert_eq!(age(t, after(300)), "5 min ago");
    assert_eq!(age(t, after(7_200)), "2 h ago");
    assert_eq!(age(t, after(86_400)), "1 day ago");
    assert_eq!(age(t, after(3 * 86_400)), "3 days ago");
    assert_eq!(size(512), "1 KB");
    assert_eq!(size(3 * 1024 * 1024 + 400 * 1024), "3.4 MB");
}
