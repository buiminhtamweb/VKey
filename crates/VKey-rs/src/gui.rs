use eframe::egui;
#[cfg(target_os = "linux")]
use gtk::{
    glib::{
        self,
        object::ObjectExt,
        translate::{IntoGlib, ToGlibPtr, from_glib_full},
    },
    prelude::*,
};
use std::sync::mpsc::{Receiver, Sender};
#[cfg(not(target_os = "linux"))]
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuItem, Submenu},
};
use vietnamese_core::{Charset, EngineConfig, InputMethod};

use crate::{AppMessage, GuiMessage};

// Thread-safe wrapper for MenuItem to allow updating tray menu items from background threads on Windows
#[cfg(not(target_os = "linux"))]
struct SendMenuItem(MenuItem);
#[cfg(not(target_os = "linux"))]
unsafe impl Send for SendMenuItem {}
#[cfg(not(target_os = "linux"))]
unsafe impl Sync for SendMenuItem {}

#[cfg(target_os = "windows")]
fn force_show_window() {
    unsafe {
        let title: Vec<u16> = "VKey Settings\0".encode_utf16().collect();
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
            std::ptr::null(),
            title.as_ptr(),
        );
        if hwnd != 0 {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_RESTORE,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "linux")))]
fn force_show_window() {}

#[cfg(not(target_os = "linux"))]
impl std::ops::Deref for SendMenuItem {
    type Target = MenuItem;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Thread-safe wrapper for TrayIcon to allow setting tray icons from background threads on Windows
#[cfg(not(target_os = "linux"))]
pub struct SendTrayIcon(TrayIcon);
#[cfg(not(target_os = "linux"))]
unsafe impl Send for SendTrayIcon {}
#[cfg(not(target_os = "linux"))]
unsafe impl Sync for SendTrayIcon {}

#[cfg(not(target_os = "linux"))]
impl std::ops::Deref for SendTrayIcon {
    type Target = TrayIcon;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct AppGui {
    config: EngineConfig,
    tx: Sender<AppMessage>,
    gui_rx: Receiver<GuiMessage>,
    #[cfg(not(target_os = "linux"))]
    tray_icon: Option<std::sync::Arc<SendTrayIcon>>,
    #[cfg(not(target_os = "linux"))]
    menu_enabled: MenuItem,
    #[cfg(not(target_os = "linux"))]
    menu_telex: MenuItem,
    #[cfg(not(target_os = "linux"))]
    menu_vni: MenuItem,
    #[cfg(not(target_os = "linux"))]
    menu_unicode: MenuItem,
    #[cfg(not(target_os = "linux"))]
    menu_tcvn3: MenuItem,
    #[cfg(not(target_os = "linux"))]
    menu_vni_charset: MenuItem,
    window_visible: bool,
    exit_requested: bool,
    shared_config: std::sync::Arc<std::sync::Mutex<EngineConfig>>,
    #[cfg(target_os = "linux")]
    tray_channel: Sender<EngineConfig>,
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct LinuxTrayControls {
    status_icon: glib::Object,
    menu_enabled: gtk::MenuItem,
    menu_telex: gtk::MenuItem,
    menu_vni: gtk::MenuItem,
    menu_unicode: gtk::MenuItem,
    menu_tcvn3: gtk::MenuItem,
    menu_vni_charset: gtk::MenuItem,
}

#[cfg(target_os = "linux")]
impl LinuxTrayControls {
    fn sync(&self, config: &EngineConfig) {
        self.menu_enabled.set_label(if config.enabled {
            "✓ Bật tiếng Việt"
        } else {
            "  Bật tiếng Việt"
        });
        self.menu_telex
            .set_label(if config.input_method == InputMethod::Telex {
                "✓ Telex"
            } else {
                "  Telex"
            });
        self.menu_vni
            .set_label(if config.input_method == InputMethod::Vni {
                "✓ VNI"
            } else {
                "  VNI"
            });
        self.menu_unicode
            .set_label(if config.charset == Charset::Unicode {
                "✓ Unicode"
            } else {
                "  Unicode"
            });
        self.menu_tcvn3
            .set_label(if config.charset == Charset::Tcvn3 {
                "✓ TCVN3 (ABC)"
            } else {
                "  TCVN3 (ABC)"
            });
        self.menu_vni_charset
            .set_label(if config.charset == Charset::Vni {
                "✓ VNI Windows"
            } else {
                "  VNI Windows"
            });
        set_linux_status_icon_pixbuf(&self.status_icon, config.enabled);
    }
}

#[cfg(target_os = "linux")]
fn publish_linux_tray_config(
    config: &EngineConfig,
    controls: &LinuxTrayControls,
    tx: &Sender<AppMessage>,
    ctx: &egui::Context,
) {
    crate::config_store::save_config(config);
    let _ = tx.send(AppMessage::UpdateConfig(config.clone()));
    controls.sync(config);
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

#[cfg(target_os = "linux")]
fn build_linux_menu_item(label: &str) -> gtk::MenuItem {
    gtk::MenuItem::with_label(label)
}

#[cfg(target_os = "linux")]
fn build_linux_tray_menu(
    config: &EngineConfig,
) -> (
    gtk::Menu,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
    gtk::MenuItem,
) {
    let menu_enabled = build_linux_menu_item(if config.enabled {
        "✓ Bật tiếng Việt"
    } else {
        "  Bật tiếng Việt"
    });
    let menu_telex = build_linux_menu_item(if config.input_method == InputMethod::Telex {
        "✓ Telex"
    } else {
        "  Telex"
    });
    let menu_vni = build_linux_menu_item(if config.input_method == InputMethod::Vni {
        "✓ VNI"
    } else {
        "  VNI"
    });
    let menu_unicode = build_linux_menu_item(if config.charset == Charset::Unicode {
        "✓ Unicode"
    } else {
        "  Unicode"
    });
    let menu_tcvn3 = build_linux_menu_item(if config.charset == Charset::Tcvn3 {
        "✓ TCVN3 (ABC)"
    } else {
        "  TCVN3 (ABC)"
    });
    let menu_vni_charset = build_linux_menu_item(if config.charset == Charset::Vni {
        "✓ VNI Windows"
    } else {
        "  VNI Windows"
    });
    let menu_settings = build_linux_menu_item("Hiển thị cài đặt");
    let menu_exit = build_linux_menu_item("Thoát");

    let method_root = build_linux_menu_item("Kiểu gõ");
    let method_menu = gtk::Menu::new();
    method_menu.append(&menu_telex);
    method_menu.append(&menu_vni);
    method_root.set_submenu(Some(&method_menu));

    let charset_root = build_linux_menu_item("Bảng mã");
    let charset_menu = gtk::Menu::new();
    charset_menu.append(&menu_unicode);
    charset_menu.append(&menu_tcvn3);
    charset_menu.append(&menu_vni_charset);
    charset_root.set_submenu(Some(&charset_menu));

    let tray_menu = gtk::Menu::new();
    tray_menu.append(&menu_enabled);
    tray_menu.append(&method_root);
    tray_menu.append(&charset_root);
    tray_menu.append(&menu_settings);
    tray_menu.append(&menu_exit);
    tray_menu.show_all();

    (
        tray_menu,
        menu_enabled,
        menu_telex,
        menu_vni,
        menu_unicode,
        menu_tcvn3,
        menu_vni_charset,
        menu_settings,
        menu_exit,
    )
}

#[cfg(target_os = "linux")]
fn create_linux_status_icon(is_vietnamese: bool) -> glib::Object {
    let pixbuf = generate_tray_pixbuf(is_vietnamese);
    let status_icon = unsafe {
        let raw = gtk::ffi::gtk_status_icon_new_from_pixbuf(pixbuf.to_glib_none().0);
        from_glib_full::<_, glib::Object>(raw.cast())
    };
    unsafe {
        gtk::ffi::gtk_status_icon_set_tooltip_text(
            status_icon_ptr(&status_icon),
            "VKey - Bộ gõ Tiếng Việt".to_glib_none().0,
        );
        gtk::ffi::gtk_status_icon_set_title(status_icon_ptr(&status_icon), "VKey".to_glib_none().0);
        gtk::ffi::gtk_status_icon_set_visible(status_icon_ptr(&status_icon), true.into_glib());
    }
    status_icon
}

#[cfg(target_os = "linux")]
fn status_icon_ptr(status_icon: &glib::Object) -> *mut gtk::ffi::GtkStatusIcon {
    status_icon.as_ptr().cast()
}

#[cfg(target_os = "linux")]
fn set_linux_status_icon_pixbuf(status_icon: &glib::Object, is_vietnamese: bool) {
    let pixbuf = generate_tray_pixbuf(is_vietnamese);
    unsafe {
        gtk::ffi::gtk_status_icon_set_from_pixbuf(
            status_icon_ptr(status_icon),
            pixbuf.to_glib_none().0,
        );
    }
}

#[cfg(target_os = "linux")]
fn hide_linux_status_icon(status_icon: &glib::Object) {
    unsafe {
        gtk::ffi::gtk_status_icon_set_visible(status_icon_ptr(status_icon), false.into_glib());
    }
}

#[cfg(target_os = "linux")]
fn popup_button_and_time(values: &[glib::Value]) -> (u32, u32) {
    let button = values
        .get(1)
        .and_then(|value| value.get::<u32>().ok())
        .unwrap_or(0);
    let activate_time = values
        .get(2)
        .and_then(|value| value.get::<u32>().ok())
        .unwrap_or_else(|| unsafe { gtk::ffi::gtk_get_current_event_time() });
    (button, activate_time)
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn connect_linux_tray_handlers(
    current_config: std::rc::Rc<std::cell::RefCell<EngineConfig>>,
    controls: LinuxTrayControls,
    tray_menu: gtk::Menu,
    menu_enabled: &gtk::MenuItem,
    menu_telex: &gtk::MenuItem,
    menu_vni: &gtk::MenuItem,
    menu_unicode: &gtk::MenuItem,
    menu_tcvn3: &gtk::MenuItem,
    menu_vni_charset: &gtk::MenuItem,
    menu_settings: &gtk::MenuItem,
    menu_exit: &gtk::MenuItem,
    tx: &Sender<AppMessage>,
    gui_tx: &Sender<GuiMessage>,
    ctx: &egui::Context,
) {
    let toggle_enabled = {
        let current_config = current_config.clone();
        let controls = controls.clone();
        let tx = tx.clone();
        let ctx = ctx.clone();
        move || {
            let mut config = current_config.borrow().clone();
            config.enabled = !config.enabled;
            *current_config.borrow_mut() = config.clone();
            publish_linux_tray_config(&config, &controls, &tx, &ctx);
        }
    };

    let status_icon = controls.status_icon.clone();
    status_icon.connect_local("activate", false, move |_| {
        toggle_enabled();
        None
    });

    let menu = tray_menu.clone();
    controls
        .status_icon
        .connect_local("popup-menu", false, move |values| {
            let (button, activate_time) = popup_button_and_time(values);
            menu.popup_easy(button, activate_time);
            None
        });

    let connect_config_item =
        |item: &gtk::MenuItem, mutate: Box<dyn Fn(&mut EngineConfig) + 'static>| {
            let current_config = current_config.clone();
            let controls = controls.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            item.connect_activate(move |_| {
                let mut config = current_config.borrow().clone();
                mutate(&mut config);
                *current_config.borrow_mut() = config.clone();
                publish_linux_tray_config(&config, &controls, &tx, &ctx);
            });
        };

    connect_config_item(
        menu_enabled,
        Box::new(|config| config.enabled = !config.enabled),
    );
    connect_config_item(
        menu_telex,
        Box::new(|config| config.input_method = InputMethod::Telex),
    );
    connect_config_item(
        menu_vni,
        Box::new(|config| config.input_method = InputMethod::Vni),
    );
    connect_config_item(
        menu_unicode,
        Box::new(|config| config.charset = Charset::Unicode),
    );
    connect_config_item(
        menu_tcvn3,
        Box::new(|config| config.charset = Charset::Tcvn3),
    );
    connect_config_item(
        menu_vni_charset,
        Box::new(|config| config.charset = Charset::Vni),
    );

    let gui_tx_settings = gui_tx.clone();
    let ctx_settings = ctx.clone();
    menu_settings.connect_activate(move |_| {
        let _ = gui_tx_settings.send(GuiMessage::ShowSettingsWindow);
        ctx_settings.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx_settings.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx_settings.request_repaint_of(egui::ViewportId::ROOT);
    });

    let tx_exit = tx.clone();
    let gui_tx_exit = gui_tx.clone();
    let ctx_exit = ctx.clone();
    let status_icon = controls.status_icon.clone();
    menu_exit.connect_activate(move |_| {
        hide_linux_status_icon(&status_icon);
        let _ = tx_exit.send(AppMessage::Exit);
        let _ = gui_tx_exit.send(GuiMessage::ExitRequested);
        ctx_exit.request_repaint_of(egui::ViewportId::ROOT);
        gtk::main_quit();
    });
}

impl AppGui {
    #[cfg(target_os = "linux")]
    pub fn new(
        config: EngineConfig,
        tx: Sender<AppMessage>,
        gui_rx: Receiver<GuiMessage>,
        gui_tx: Sender<GuiMessage>,
        ctx: egui::Context,
    ) -> Self {
        let (tray_tx, tray_rx) = std::sync::mpsc::channel::<EngineConfig>();

        let config_clone = config.clone();
        let tx_clone = tx.clone();
        let gui_tx_clone = gui_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            if let Err(err) = gtk::init() {
                tracing::error!("Failed to initialize GTK on background thread: {}", err);
                return;
            }

            let (
                tray_menu,
                menu_enabled,
                menu_telex,
                menu_vni,
                menu_unicode,
                menu_tcvn3,
                menu_vni_charset,
                menu_settings,
                menu_exit,
            ) = build_linux_tray_menu(&config_clone);
            let status_icon = create_linux_status_icon(config_clone.enabled);
            let controls = LinuxTrayControls {
                status_icon,
                menu_enabled: menu_enabled.clone(),
                menu_telex: menu_telex.clone(),
                menu_vni: menu_vni.clone(),
                menu_unicode: menu_unicode.clone(),
                menu_tcvn3: menu_tcvn3.clone(),
                menu_vni_charset: menu_vni_charset.clone(),
            };
            let current_config = std::rc::Rc::new(std::cell::RefCell::new(config_clone.clone()));

            connect_linux_tray_handlers(
                current_config.clone(),
                controls.clone(),
                tray_menu,
                &menu_enabled,
                &menu_telex,
                &menu_vni,
                &menu_unicode,
                &menu_tcvn3,
                &menu_vni_charset,
                &menu_settings,
                &menu_exit,
                &tx_clone,
                &gui_tx_clone,
                &ctx_clone,
            );

            gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                while let Ok(new_cfg) = tray_rx.try_recv() {
                    *current_config.borrow_mut() = new_cfg.clone();
                    controls.sync(&new_cfg);
                }

                gtk::glib::ControlFlow::Continue
            });

            gtk::main();
        });

        let shared_config = std::sync::Arc::new(std::sync::Mutex::new(config.clone()));

        Self {
            config,
            tx,
            gui_rx,
            window_visible: true,
            exit_requested: false,
            shared_config,
            tray_channel: tray_tx,
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(
        config: EngineConfig,
        tx: Sender<AppMessage>,
        gui_rx: Receiver<GuiMessage>,
        gui_tx: Sender<GuiMessage>,
        ctx: egui::Context,
    ) -> Self {
        // 1. Create Tray Menu Items
        let menu_enabled = MenuItem::with_id(
            "enabled",
            if config.enabled {
                "✓ Bật tiếng Việt"
            } else {
                "  Bật tiếng Việt"
            },
            true,
            None,
        );
        let menu_telex = MenuItem::with_id(
            "method_telex",
            if config.input_method == InputMethod::Telex {
                "✓ Telex"
            } else {
                "  Telex"
            },
            true,
            None,
        );
        let menu_vni = MenuItem::with_id(
            "method_vni",
            if config.input_method == InputMethod::Vni {
                "✓ VNI"
            } else {
                "  VNI"
            },
            true,
            None,
        );
        let menu_unicode = MenuItem::with_id(
            "charset_unicode",
            if config.charset == Charset::Unicode {
                "✓ Unicode"
            } else {
                "  Unicode"
            },
            true,
            None,
        );
        let menu_tcvn3 = MenuItem::with_id(
            "charset_tcvn3",
            if config.charset == Charset::Tcvn3 {
                "✓ TCVN3 (ABC)"
            } else {
                "  TCVN3 (ABC)"
            },
            true,
            None,
        );
        let menu_vni_charset = MenuItem::with_id(
            "charset_vni",
            if config.charset == Charset::Vni {
                "✓ VNI Windows"
            } else {
                "  VNI Windows"
            },
            true,
            None,
        );

        // Build Submenu for Input Methods
        let method_submenu = Submenu::with_id("method", "Kiểu gõ", true);
        method_submenu.append(&menu_telex).unwrap();
        method_submenu.append(&menu_vni).unwrap();

        // Build Submenu for Charsets
        let charset_submenu = Submenu::with_id("charset", "Bảng mã", true);
        charset_submenu.append(&menu_unicode).unwrap();
        charset_submenu.append(&menu_tcvn3).unwrap();
        charset_submenu.append(&menu_vni_charset).unwrap();

        // Build Tray Menu
        let tray_menu = Menu::new();
        tray_menu.append(&menu_enabled).unwrap();
        tray_menu.append(&method_submenu).unwrap();
        tray_menu.append(&charset_submenu).unwrap();
        tray_menu
            .append(&MenuItem::with_id(
                "settings",
                "Hiển thị cài đặt",
                true,
                None,
            ))
            .unwrap();
        tray_menu
            .append(&MenuItem::with_id("exit", "Thoát", true, None))
            .unwrap();

        // 2. Create Tray Icon
        let icon = generate_tray_icon(config.enabled);
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_menu_on_left_click(false)
            .with_tooltip("VKey - Bộ gõ Tiếng Việt")
            .with_icon(icon)
            .build()
            .ok();

        let tray_icon = tray_icon.map(|t| std::sync::Arc::new(SendTrayIcon(t)));

        // 3. Shared thread-safe state for background tray thread
        let shared_config = std::sync::Arc::new(std::sync::Mutex::new(config.clone()));
        let shared_config_clone = shared_config.clone();

        let tx_clone = tx.clone();
        let gui_tx_clone = gui_tx.clone();
        let ctx_clone = ctx.clone();

        let menu_enabled_send = SendMenuItem(menu_enabled.clone());
        let menu_telex_send = SendMenuItem(menu_telex.clone());
        let menu_vni_send = SendMenuItem(menu_vni.clone());
        let menu_unicode_send = SendMenuItem(menu_unicode.clone());
        let menu_tcvn3_send = SendMenuItem(menu_tcvn3.clone());
        let menu_vni_charset_send = SendMenuItem(menu_vni_charset.clone());
        let shared_tray_clone = tray_icon.clone();

        std::thread::spawn(move || {
            let tray_event_receiver = tray_icon::TrayIconEvent::receiver();
            let menu_event_receiver = tray_icon::menu::MenuEvent::receiver();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));

                while let Ok(event) = tray_event_receiver.try_recv() {
                    match event {
                        tray_icon::TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            button_state: tray_icon::MouseButtonState::Up,
                            ..
                        }
                        | tray_icon::TrayIconEvent::DoubleClick {
                            button: tray_icon::MouseButton::Left,
                            ..
                        } => {
                            let mut cfg = shared_config_clone.lock().unwrap().clone();
                            cfg.enabled = !cfg.enabled;

                            // Save to file
                            crate::config_store::save_config(&cfg);
                            // Notify background thread
                            let _ = tx_clone.send(AppMessage::UpdateConfig(cfg.clone()));
                            // Notify GUI thread
                            let _ = gui_tx_clone.send(GuiMessage::StateChanged(cfg.clone()));
                            // Update shared lock
                            *shared_config_clone.lock().unwrap() = cfg.clone();
                            // Update tray icon immediately
                            if let Some(tray) = shared_tray_clone.as_ref() {
                                let icon = generate_tray_icon(cfg.enabled);
                                let _ = tray.set_icon(Some(icon));
                            }

                            // Synchronize tray menu checkmark immediately
                            menu_enabled_send.set_text(if cfg.enabled {
                                "✓ Bật tiếng Việt"
                            } else {
                                "  Bật tiếng Việt"
                            });

                            // Wake up winit to update GUI window immediately
                            ctx_clone.request_repaint_of(egui::ViewportId::ROOT);
                        }
                        _ => {}
                    }
                }

                if let Ok(event) = menu_event_receiver.try_recv() {
                    let mut cfg = shared_config_clone.lock().unwrap().clone();
                    let mut changed = false;
                    let mut exit_app = false;

                    match event.id.0.as_str() {
                        "enabled" => {
                            cfg.enabled = !cfg.enabled;
                            changed = true;
                        }
                        "method_telex" => {
                            cfg.input_method = InputMethod::Telex;
                            changed = true;
                        }
                        "method_vni" => {
                            cfg.input_method = InputMethod::Vni;
                            changed = true;
                        }
                        "charset_unicode" => {
                            cfg.charset = Charset::Unicode;
                            changed = true;
                        }
                        "charset_tcvn3" => {
                            cfg.charset = Charset::Tcvn3;
                            changed = true;
                        }
                        "charset_vni" => {
                            cfg.charset = Charset::Vni;
                            changed = true;
                        }
                        "settings" => {
                            let _ = gui_tx_clone.send(GuiMessage::ShowSettingsWindow);
                            force_show_window();
                            ctx_clone.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx_clone.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx_clone.request_repaint_of(egui::ViewportId::ROOT);
                        }
                        "exit" => {
                            exit_app = true;
                        }
                        _ => {}
                    }

                    if exit_app {
                        let _ = tx_clone.send(AppMessage::Exit);
                        let _ = gui_tx_clone.send(GuiMessage::ExitRequested);
                        ctx_clone.request_repaint_of(egui::ViewportId::ROOT);
                        break;
                    }

                    if changed {
                        // 1. Save to disk
                        crate::config_store::save_config(&cfg);
                        // 2. Notify keyboard daemon
                        let _ = tx_clone.send(AppMessage::UpdateConfig(cfg.clone()));
                        // 3. Update shared lock
                        *shared_config_clone.lock().unwrap() = cfg.clone();
                        // 4. Notify GUI thread directly
                        let _ = gui_tx_clone.send(GuiMessage::StateChanged(cfg.clone()));

                        // 5. Update tray menu checkmarks immediately
                        menu_enabled_send.set_text(if cfg.enabled {
                            "✓ Bật tiếng Việt"
                        } else {
                            "  Bật tiếng Việt"
                        });
                        menu_telex_send.set_text(if cfg.input_method == InputMethod::Telex {
                            "✓ Telex"
                        } else {
                            "  Telex"
                        });
                        menu_vni_send.set_text(if cfg.input_method == InputMethod::Vni {
                            "✓ VNI"
                        } else {
                            "  VNI"
                        });
                        menu_unicode_send.set_text(if cfg.charset == Charset::Unicode {
                            "✓ Unicode"
                        } else {
                            "  Unicode"
                        });
                        menu_tcvn3_send.set_text(if cfg.charset == Charset::Tcvn3 {
                            "✓ TCVN3 (ABC)"
                        } else {
                            "  TCVN3 (ABC)"
                        });
                        menu_vni_charset_send.set_text(if cfg.charset == Charset::Vni {
                            "✓ VNI Windows"
                        } else {
                            "  VNI Windows"
                        });

                        // 6. Update tray icon immediately
                        if let Some(tray) = shared_tray_clone.as_ref() {
                            let icon = generate_tray_icon(cfg.enabled);
                            let _ = tray.set_icon(Some(icon));
                        }

                        ctx_clone.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            }
        });

        Self {
            config,
            tx,
            gui_rx,
            tray_icon,
            menu_enabled,
            menu_telex,
            menu_vni,
            menu_unicode,
            menu_tcvn3,
            menu_vni_charset,
            window_visible: true,
            exit_requested: false,
            shared_config,
        }
    }

    fn request_exit(&mut self, ctx: &egui::Context) {
        if self.exit_requested {
            return;
        }
        self.exit_requested = true;
        let _ = self.tx.send(AppMessage::Exit);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    fn update_config(&mut self, new_config: EngineConfig, ctx: &egui::Context) {
        #[cfg(not(target_os = "linux"))]
        let old_enabled = self.config.enabled;

        self.config = new_config.clone();

        // Update shared lock
        *self.shared_config.lock().unwrap() = self.config.clone();

        // Save to file
        crate::config_store::save_config(&self.config);

        // Notify background thread
        let _ = self.tx.send(AppMessage::UpdateConfig(self.config.clone()));

        #[cfg(target_os = "linux")]
        {
            let _ = self.tray_channel.send(self.config.clone());
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Sync tray menu states
            self.sync_menu_states();

            // Update icon if enabled state toggled
            if old_enabled != self.config.enabled {
                if let Some(tray) = &mut self.tray_icon {
                    let icon = generate_tray_icon(self.config.enabled);
                    let _ = tray.set_icon(Some(icon));
                }
            }
        }

        ctx.request_repaint();
    }

    fn apply_daemon_config(&mut self, new_config: EngineConfig, ctx: &egui::Context) {
        #[cfg(not(target_os = "linux"))]
        let enabled_changed = self.config.enabled != new_config.enabled;

        self.config = new_config;
        *self.shared_config.lock().unwrap() = self.config.clone();

        #[cfg(target_os = "linux")]
        {
            let _ = self.tray_channel.send(self.config.clone());
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.sync_menu_states();
            if enabled_changed {
                if let Some(tray) = &mut self.tray_icon {
                    let icon = generate_tray_icon(self.config.enabled);
                    let _ = tray.set_icon(Some(icon));
                }
            }
        }

        ctx.request_repaint();
    }

    #[cfg(not(target_os = "linux"))]
    fn sync_menu_states(&self) {
        self.menu_enabled.set_text(if self.config.enabled {
            "✓ Bật tiếng Việt"
        } else {
            "  Bật tiếng Việt"
        });
        self.menu_telex
            .set_text(if self.config.input_method == InputMethod::Telex {
                "✓ Telex"
            } else {
                "  Telex"
            });
        self.menu_vni
            .set_text(if self.config.input_method == InputMethod::Vni {
                "✓ VNI"
            } else {
                "  VNI"
            });
        self.menu_unicode
            .set_text(if self.config.charset == Charset::Unicode {
                "✓ Unicode"
            } else {
                "  Unicode"
            });
        self.menu_tcvn3
            .set_text(if self.config.charset == Charset::Tcvn3 {
                "✓ TCVN3 (ABC)"
            } else {
                "  TCVN3 (ABC)"
            });
        self.menu_vni_charset
            .set_text(if self.config.charset == Charset::Vni {
                "✓ VNI Windows"
            } else {
                "  VNI Windows"
            });
    }
}

impl eframe::App for AppGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Sync config if background thread changed it
        let current_shared = self.shared_config.lock().unwrap().clone();
        if current_shared != self.config {
            self.config = current_shared;
            #[cfg(not(target_os = "linux"))]
            self.sync_menu_states();
        }

        // 2. Poll for updates from the background keyboard thread
        while let Ok(msg) = self.gui_rx.try_recv() {
            match msg {
                GuiMessage::StateChanged(new_config) => {
                    self.apply_daemon_config(new_config, ctx);
                }
                GuiMessage::ShowSettingsWindow => {
                    self.window_visible = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                GuiMessage::ExitRequested => self.request_exit(ctx),
            }
        }

        // Hide window instead of closing the application when "X" is clicked
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.window_visible = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Render UI only if window is visible
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        if focused && !self.window_visible {
            self.window_visible = true;
        }

        if !self.window_visible {
            return;
        }

        let mut visuals = egui::Visuals::light();
        visuals.window_fill = egui::Color32::from_rgb(245, 247, 250);
        visuals.panel_fill = egui::Color32::from_rgb(245, 247, 250);
        visuals.override_text_color = Some(egui::Color32::BLACK); // Force black text for high contrast

        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(245, 247, 250); // Soft white-blue background
        visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::BLACK; // Force black text

        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(225, 232, 242); // Soft blue-gray for inactive widgets
        visuals.widgets.inactive.fg_stroke.color = egui::Color32::BLACK;

        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(205, 218, 238); // Hover blue
        visuals.widgets.hovered.fg_stroke.color = egui::Color32::from_rgb(0, 102, 204);

        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(13, 110, 253); // Selected/active primary blue
        visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;

        visuals.selection.bg_fill = egui::Color32::from_rgb(13, 110, 253);
        ctx.set_visuals(visuals);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);

            // Header Banner
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new("VKey")
                        .size(24.0)
                        .strong()
                        .color(egui::Color32::from_rgb(13, 110, 253)),
                );
                ui.label(
                    egui::RichText::new("Bộ gõ Tiếng Việt thế hệ mới")
                        .size(11.0)
                        .italics()
                        .color(egui::Color32::from_rgb(100, 110, 125)),
                );
                #[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
                {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("⚠️ Chế độ giả lập (macOS)")
                            .size(10.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 53, 69)),
                    );
                }
            });

            ui.add_space(8.0);

            // Active/Inactive Big Switch Button
            ui.vertical_centered(|ui| {
                let (btn_text, btn_color) = if self.config.enabled {
                    ("✔  TIẾNG VIỆT (BẬT)", egui::Color32::from_rgb(13, 110, 253))
                } else {
                    (
                        "❌  TIẾNG ANH (TẮT)",
                        egui::Color32::from_rgb(108, 117, 125),
                    )
                };

                let button = egui::Button::new(
                    egui::RichText::new(btn_text)
                        .size(15.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(btn_color)
                .rounding(20.0) // Pill shape
                .min_size(egui::vec2(220.0, 36.0));

                if ui.add(button).clicked() {
                    let mut config = self.config.clone();
                    config.enabled = !config.enabled;
                    self.update_config(config, ctx);
                }
            });

            ui.add_space(10.0);

            // Group 1: Cấu hình gõ (Input settings card)
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(238, 244, 255)) // Extremely soft premium blue background
                .rounding(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        egui::Grid::new("settings_grid")
                            .num_columns(2)
                            .spacing([12.0, 10.0])
                            .show(ui, |ui| {
                                // Row 1: Kiểu gõ
                                ui.label(
                                    egui::RichText::new("Kiểu gõ:")
                                        .strong()
                                        .size(13.0)
                                        .color(egui::Color32::BLACK),
                                );
                                ui.horizontal(|ui| {
                                    let telex_radio = ui.radio_value(
                                        &mut self.config.input_method,
                                        InputMethod::Telex,
                                        egui::RichText::new("Telex").color(egui::Color32::BLACK),
                                    );
                                    ui.add_space(10.0);
                                    let vni_radio = ui.radio_value(
                                        &mut self.config.input_method,
                                        InputMethod::Vni,
                                        egui::RichText::new("VNI").color(egui::Color32::BLACK),
                                    );

                                    if telex_radio.changed() || vni_radio.changed() {
                                        self.update_config(self.config.clone(), ctx);
                                    }
                                });
                                ui.end_row();

                                // Row 2: Bảng mã
                                ui.label(
                                    egui::RichText::new("Bảng mã:")
                                        .strong()
                                        .size(13.0)
                                        .color(egui::Color32::BLACK),
                                );
                                let mut current_charset = self.config.charset;
                                egui::ComboBox::from_id_salt("charset_combobox")
                                    .selected_text(match current_charset {
                                        Charset::Unicode => "Unicode dựng sẵn",
                                        Charset::Tcvn3 => "TCVN3 (ABC)",
                                        Charset::Vni => "VNI Windows",
                                    })
                                    .show_ui(ui, |ui| {
                                        let mut changed = false;
                                        changed |= ui
                                            .selectable_value(
                                                &mut current_charset,
                                                Charset::Unicode,
                                                "Unicode dựng sẵn",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut current_charset,
                                                Charset::Tcvn3,
                                                "TCVN3 (ABC)",
                                            )
                                            .changed();
                                        changed |= ui
                                            .selectable_value(
                                                &mut current_charset,
                                                Charset::Vni,
                                                "VNI Windows",
                                            )
                                            .changed();

                                        if changed {
                                            self.config.charset = current_charset;
                                            self.update_config(self.config.clone(), ctx);
                                        }
                                    });
                                ui.end_row();

                                // Row 3: Phím chuyển
                                ui.label(
                                    egui::RichText::new("Phím chuyển:")
                                        .strong()
                                        .size(13.0)
                                        .color(egui::Color32::BLACK),
                                );
                                ui.horizontal(|ui| {
                                    let mut current_shortcut = self.config.shortcut_key;
                                    let ctrl_shift_radio = ui.radio_value(
                                        &mut current_shortcut,
                                        vietnamese_core::config::ShortcutKey::CtrlShift,
                                        egui::RichText::new("Ctrl + Shift")
                                            .color(egui::Color32::BLACK),
                                    );
                                    ui.add_space(10.0);
                                    let alt_z_radio = ui.radio_value(
                                        &mut current_shortcut,
                                        vietnamese_core::config::ShortcutKey::AltZ,
                                        egui::RichText::new("Alt + Z").color(egui::Color32::BLACK),
                                    );

                                    if ctrl_shift_radio.changed() || alt_z_radio.changed() {
                                        self.config.shortcut_key = current_shortcut;
                                        self.update_config(self.config.clone(), ctx);
                                    }
                                });
                                ui.end_row();
                            });
                    });
                });

            ui.add_space(8.0);

            // Group 2: Hệ thống & Tùy chọn nâng cao (Advanced card)
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(240, 242, 245)) // Extremely soft light gray background
                .rounding(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let mut config_changed = false;

                        let mut startup_with_system = self.config.startup_with_system;
                        let startup_chk = ui.checkbox(
                            &mut startup_with_system,
                            egui::RichText::new("Khởi động cùng hệ thống")
                                .color(egui::Color32::BLACK),
                        );
                        if startup_chk.changed() {
                            self.config.startup_with_system = startup_with_system;
                            set_startup(startup_with_system);
                            config_changed = true;
                        }

                        let mut smart_tone = self.config.smart_tone;
                        let st_chk = ui.checkbox(
                            &mut smart_tone,
                            egui::RichText::new("Đặt dấu xoay vòng (Smart Tone)")
                                .color(egui::Color32::BLACK),
                        ).on_hover_text(
                            "Tự động đặt dấu thanh chuẩn theo quy tắc tiếng Việt hiện đại (kiểu mới).\n\
                             Ví dụ:\n\
                             - Bật (Smart): h-o-a-f -> hoà (dấu thanh đặt ở chữ 'a')\n\
                             - Tắt (Classic): h-o-a-f -> hòa (dấu thanh đặt ở chữ 'o')"
                        );
                        if st_chk.changed() {
                            self.config.smart_tone = smart_tone;
                            config_changed = true;
                        }

                        let mut restore_typing = self.config.restore_typing;
                        let rt_chk = ui.checkbox(
                            &mut restore_typing,
                            egui::RichText::new("Khôi phục phím khi gõ sai (Restore typing)")
                                .color(egui::Color32::BLACK),
                        ).on_hover_text(
                            "Cho phép gõ hai lần phím dấu để khôi phục phím chữ gốc (tránh lỗi khi gõ từ tiếng Anh).\n\
                             Ví dụ:\n\
                             - Bật: Gõ 'ss' trên chữ 'lá' -> 'las'\n\
                             - Tắt: Gõ 'ss' trên chữ 'lá' -> 'lá'"
                        );
                        if rt_chk.changed() {
                            self.config.restore_typing = restore_typing;
                            config_changed = true;
                        }

                        if config_changed {
                            self.update_config(self.config.clone(), ctx);
                        }
                    });
                });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(8.0);

            // Footer actions
            ui.horizontal(|ui| {
                let hide_btn =
                    egui::Button::new(egui::RichText::new("Ẩn xuống khay hệ thống").size(12.0))
                        .rounding(4.0)
                        .fill(egui::Color32::from_rgb(225, 235, 250)); // soft blue outline button

                if ui.add(hide_btn).clicked() {
                    self.window_visible = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let exit_btn =
                        egui::Button::new(egui::RichText::new("Thoát hoàn toàn").size(12.0))
                            .rounding(4.0)
                            .fill(egui::Color32::from_rgb(255, 230, 230)); // soft red outline button

                    if ui.add(exit_btn).clicked() {
                        self.request_exit(ctx);
                    }
                });
            });
        });
    }
}

impl Drop for AppGui {
    fn drop(&mut self) {
        let _ = self.tx.send(AppMessage::Exit);
    }
}

#[cfg(target_os = "windows")]
fn set_startup(enabled: bool) {
    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy().into_owned();
        std::thread::spawn(move || {
            if enabled {
                let _ = std::process::Command::new("reg")
                    .args([
                        "add",
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                        "/v",
                        "VKey",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &format!("\"{}\"", exe_str),
                        "/f",
                    ])
                    .output();
            } else {
                let _ = std::process::Command::new("reg")
                    .args([
                        "delete",
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                        "/v",
                        "VKey",
                        "/f",
                    ])
                    .output();
            }
        });
    }
}

#[cfg(not(target_os = "windows"))]
fn set_startup(_enabled: bool) {}

// Generate the tray icon dynamically by drawing into an RGBA buffer
fn generate_tray_pixels(is_vietnamese: bool) -> (Vec<u8>, usize, usize) {
    let width = 32;
    let height = 32;
    let mut pixels = vec![0u8; width * height * 4];

    let center_x = 16.0;
    let center_y = 16.0;
    let radius = 14.0;

    let bg_color = if is_vietnamese {
        [13, 110, 253, 255] // Blue (#0d6efd)
    } else {
        [108, 117, 125, 255] // Gray (#6c757d)
    };

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32) - center_x;
            let dy = (y as f32) - center_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = (y * width + x) * 4;

            if dist <= radius {
                pixels[idx] = bg_color[0];
                pixels[idx + 1] = bg_color[1];
                pixels[idx + 2] = bg_color[2];
                pixels[idx + 3] = bg_color[3];
            }
        }
    }

    let fg = [255, 255, 255, 255]; // White

    let draw_pixel = |pixels: &mut [u8], px: i32, py: i32| {
        if (0..32).contains(&px) && (0..32).contains(&py) {
            let idx = ((py * 32 + px) * 4) as usize;
            pixels[idx] = fg[0];
            pixels[idx + 1] = fg[1];
            pixels[idx + 2] = fg[2];
            pixels[idx + 3] = fg[3];
        }
    };

    if is_vietnamese {
        // Draw letter V
        for t in 0..=14 {
            let f = t as f32 / 14.0;
            for dx in -1..=1 {
                draw_pixel(
                    &mut pixels,
                    (10.0 + f * 6.0) as i32 + dx,
                    (9.0 + f * 14.0) as i32,
                );
                draw_pixel(
                    &mut pixels,
                    (22.0 - f * 6.0) as i32 + dx,
                    (9.0 + f * 14.0) as i32,
                );
            }
        }
    } else {
        // Draw letter E
        // Vertical spine: x=11, y=9..=23
        for y in 9..=23 {
            for dx in 0..=2 {
                draw_pixel(&mut pixels, 11 + dx, y);
            }
        }
        // Top bar: y=9, x=11..=21
        for x in 11..=21 {
            for dy in 0..=2 {
                draw_pixel(&mut pixels, x, 9 + dy);
            }
        }
        // Middle bar: y=16, x=11..=18
        for x in 11..=18 {
            for dy in 0..=1 {
                draw_pixel(&mut pixels, x, 16 + dy);
            }
        }
        // Bottom bar: y=23, x=11..=21
        for x in 11..=21 {
            for dy in -2..=0 {
                draw_pixel(&mut pixels, x, 23 + dy);
            }
        }
    }

    (pixels, width, height)
}

#[cfg(not(target_os = "linux"))]
fn generate_tray_icon(is_vietnamese: bool) -> tray_icon::Icon {
    let (pixels, width, height) = generate_tray_pixels(is_vietnamese);
    tray_icon::Icon::from_rgba(pixels, width as u32, height as u32).unwrap()
}

#[cfg(target_os = "linux")]
fn generate_tray_pixbuf(is_vietnamese: bool) -> gtk::gdk_pixbuf::Pixbuf {
    let (pixels, width, height) = generate_tray_pixels(is_vietnamese);
    gtk::gdk_pixbuf::Pixbuf::from_mut_slice(
        pixels,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width as i32,
        height as i32,
        (width * 4) as i32,
    )
}

pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[allow(unused_assignments)]
    let mut font_data = None;

    #[cfg(target_os = "windows")]
    {
        font_data = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf")
            .or_else(|_| std::fs::read("C:\\Windows\\Fonts\\arial.ttf"))
            .ok();
    }
    #[cfg(target_os = "linux")]
    {
        font_data = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf"))
            .ok();
    }
    #[cfg(target_os = "macos")]
    {
        font_data = std::fs::read("/System/Library/Fonts/Helvetica.ttc")
            .or_else(|_| std::fs::read("/Library/Fonts/Arial.ttf"))
            .ok();
    }

    if let Some(data) = font_data {
        fonts.font_data.insert(
            "vietnamese_font".to_owned(),
            egui::FontData::from_owned(data),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "vietnamese_font".to_owned());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("vietnamese_font".to_owned());
    }

    ctx.set_fonts(fonts);
}
