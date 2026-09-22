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

#[test]
#[ignore = "requires an X11 session"]
fn test_premapped_table_initialization_and_cleanup() {
    let backend = X11KeyboardBackend::new().expect("connect to X11 and pre-map table");
    drop(backend);
}

#[test]
#[ignore = "requires an X11 session and a disposable focused text field"]
fn test_replace_complex_words() {
    assert_eq!(
        std::env::var("VKey_X11_INJECTION_TEST").as_deref(),
        Ok("1"),
        "set VKey_X11_INJECTION_TEST=1 only after focusing a disposable text field"
    );

    let mut backend = X11KeyboardBackend::new().expect("connect to X11");
    let mut injector = backend.text_injector();
    assert!(injector.current_target().expect("query focus").is_some());

    // Test "đồng" replacement: "d" -> "đ"
    injector.insert_text("d").expect("insert d");
    injector.replace_text(1, "đ").expect("replace d with đ");

    // Test "Cánh đồng bất tận, rược đuổi"
    injector
        .insert_text(" — Cánh đồng bất tận, rược đuổi")
        .expect("insert sentence");
}

#[test]
#[ignore = "requires X11 and running Google Chrome"]
fn test_detect_browser() {
    let mut backend = match X11KeyboardBackend::new() {
        Ok(b) => b,
        Err(_) => return, // no X11 display
    };
    let chrome_win_out = std::process::Command::new("xdotool")
        .args(["search", "--onlyvisible", "--class", "google-chrome"])
        .output();
    let win_id_u32 = chrome_win_out
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().and_then(|l| l.trim().parse::<u32>().ok())
        })
        .unwrap_or(81788955);

    let is_browser = backend.is_browser_window(win_id_u32);
    println!("Window {} is_browser: {}", win_id_u32, is_browser);
    assert!(is_browser);
}

#[test]
#[ignore = "requires X11 and running Google Chrome"]
fn test_chrome_omnibox_replace() {
    let mut backend = match X11KeyboardBackend::new() {
        Ok(b) => b,
        Err(_) => return,
    };
    let chrome_win_out = std::process::Command::new("xdotool")
        .args(["search", "--onlyvisible", "--class", "google-chrome"])
        .output();
    let win_id = chrome_win_out
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        })
        .unwrap_or_else(|| "81788955".to_string());

    for (first_char, target_char) in [("d", "đ"), ("o", "ô"), ("o", "ơ"), ("e", "ẹ")] {
        // Activate Chrome and focus Omnibox
        let _ = std::process::Command::new("xdotool")
            .args(["windowactivate", "--sync", &win_id, "key", "ctrl+l"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Clear Omnibox
        let _ = std::process::Command::new("xdotool")
            .args(["key", "BackSpace"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut injector = backend.text_injector();
        // Type first char
        injector.insert_text(first_char).expect("insert first char");
        // Wait for Chrome to trigger inline autocomplete
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Now replace first char with target char
        injector.replace_text(1, target_char).expect("replace char");
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Copy omnibox content
        let _ = std::process::Command::new("xdotool")
            .args(["key", "ctrl+a", "ctrl+c"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let output = std::process::Command::new("xclip")
            .args(["-o", "-selection", "clipboard"])
            .output()
            .expect("xclip");
        let content = String::from_utf8_lossy(&output.stdout);
        println!(
            "Case {} -> {}: got {:?}",
            first_char,
            target_char,
            content.trim()
        );
        assert_eq!(content.trim(), target_char);
    }
}

#[test]
fn test_moiwf_live() {
    let mut backend = match X11KeyboardBackend::new() {
        Ok(b) => b,
        Err(_) => return,
    };
    let chrome_win_out = std::process::Command::new("xdotool")
        .args(["search", "--onlyvisible", "--class", "google-chrome"])
        .output();
    let win_id = chrome_win_out
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        })
        .unwrap_or_else(|| "81788955".to_string());

    for (word_keys, expected) in [
        (vec![("m", 0, "m"), ("o", 0, "o"), ("i", 0, "i"), ("w", 2, "ơi"), ("f", 2, "ời")], "mời"),
        (vec![("t", 0, "t"), ("h", 0, "h"), ("o", 0, "o"), ("i", 0, "i"), ("w", 2, "ơi"), ("f", 2, "ời")], "thời"),
        (vec![("k", 0, "k"), ("y", 0, "y"), ("f", 1, "ỳ")], "kỳ"),
    ] {
        // Activate Chrome and focus Omnibox
        let _ = std::process::Command::new("xdotool")
            .args(["windowactivate", "--sync", &win_id, "key", "ctrl+l"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Clear Omnibox
        let _ = std::process::Command::new("xdotool")
            .args(["key", "BackSpace"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut injector = backend.text_injector();
        for (_input, del, rep) in word_keys {
            if del == 0 {
                injector.insert_text(rep).expect("insert");
            } else {
                injector.replace_text(del, rep).expect("replace");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Copy omnibox content
        let _ = std::process::Command::new("xdotool")
            .args(["key", "ctrl+a", "ctrl+c"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let output = std::process::Command::new("xclip")
            .args(["-o", "-selection", "clipboard"])
            .output()
            .expect("xclip");
        let content = String::from_utf8_lossy(&output.stdout);
        println!("Omnibox content for expected {:?}: {:?}", expected, content.trim());
        assert_eq!(content.trim(), expected);
    }
}
