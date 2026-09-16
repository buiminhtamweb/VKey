#![cfg(target_os = "linux")]

use keyboard_linux::{TextInjector, X11KeyboardBackend};

/// This deliberately types into the currently focused X11 window. Keep it
/// ignored so headless CI and normal `cargo test` runs remain side-effect free.
#[test]
#[ignore = "requires an X11 session and a disposable focused text field"]
fn injects_unicode_into_an_opt_in_x11_target() {
    assert_eq!(
        std::env::var("VKey_X11_INJECTION_TEST").as_deref(),
        Ok("1"),
        "set VKey_X11_INJECTION_TEST=1 only after focusing a disposable text field"
    );

    let mut backend = X11KeyboardBackend::new().expect("connect to X11");
    let mut injector = backend.text_injector();
    assert!(injector.current_target().expect("query focus").is_some());
    injector
        .insert_text("được được — Tiếng Việt — Đặng Nguyễn")
        .expect("inject Unicode through XTEST");
    injector
        .delete_previous_graphemes(6)
        .expect("delete six visible graphemes");
}

#[test]
#[ignore = "requires an X11 session and a disposable focused text field"]
fn test_replace_buif() {
    assert_eq!(
        std::env::var("VKey_X11_INJECTION_TEST").as_deref(),
        Ok("1"),
        "set VKey_X11_INJECTION_TEST=1 only after focusing a disposable text field"
    );

    let mut backend = X11KeyboardBackend::new().expect("connect to X11");
    let mut injector = backend.text_injector();
    assert!(injector.current_target().expect("query focus").is_some());
    injector.insert_text("bui").expect("insert bui");
    injector.replace_text(2, "ùi").expect("replace ui with ùi");
}

#[test]
#[ignore = "requires an X11 session and a disposable focused text field"]
fn test_tam_dot() {
    assert_eq!(
        std::env::var("VKey_X11_INJECTION_TEST").as_deref(),
        Ok("1"),
        "set VKey_X11_INJECTION_TEST=1 only after focusing a disposable text field"
    );

    let mut backend = X11KeyboardBackend::new().expect("connect to X11");
    let mut injector = backend.text_injector();
    assert!(injector.current_target().expect("query focus").is_some());
    injector.insert_text("Tâm.").expect("insert Tâm.");
}
