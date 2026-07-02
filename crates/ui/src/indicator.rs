use anyhow::{Context as _, Result};
use tao::{
    dpi::PhysicalSize,
    event_loop::EventLoop,
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            VIRTUAL_KEY,
        },
        WindowsAndMessaging::{
            SetWindowLongW, GWL_EXSTYLE, GWL_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_POPUP,
        },
    },
};
use wry::{WebView, WebViewBuilder};

use crate::UserEvent;

pub fn create_indicator_window(event_loop: &EventLoop<UserEvent>) -> Result<Window> {
    let window = WindowBuilder::new()
        .with_decorations(false)
        .with_title("Indicator")
        .with_focused(false)
        // .with_visible(false)
        .with_undecorated_shadow(false)
        .with_transparent(true)
        .build(event_loop)
        .context("Failed to create window")?;

    window.set_inner_size(PhysicalSize::new(90.0, 90.0));

    let hwnd = window.hwnd() as *mut std::ffi::c_void;

    // set extended window style
    // https://docs.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
    // https://docs.microsoft.com/en-us/windows/win32/winmsg/window-styles
    unsafe {
        let exnewstyle = WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0;
        SetWindowLongW(HWND(hwnd), GWL_EXSTYLE, exnewstyle as i32);

        let style = WS_POPUP.0;
        SetWindowLongW(HWND(hwnd), GWL_STYLE, style as i32);
    };

    Ok(window)
}

pub fn create_indicator_webview(window: &Window) -> Result<WebView> {
    let webview = WebViewBuilder::new()
        .with_transparent(true)
        .with_html(
            r##"
        <html>
            <head>
                <style>
                    body, html {
                        overscroll-behavior: none;
                    }
                    body {
                        margin: 0;
                        padding: 7px;
                        filter: drop-shadow(3px 3px 3px rgba(0, 0, 0, 0.1));
                    }
                    main {
                        width: 100%;
                        height: 100%;
                        border: 1px solid #2CB5FF;
                        border-radius: 8px;
                        background-color: #FFFFFF;
                        box-sizing: border-box;
                        display: flex;
                        justify-content: center;
                        align-items: center;
                        cursor: default;
                        user-select: none;
                    }
                    #context-menu {
                        display: none;
                        position: fixed;
                        background: #FFFFFF;
                        border: 1px solid #E4E4E4;
                        border-radius: 6px;
                        box-shadow: 0 4px 12px rgba(0,0,0,0.15);
                        padding: 4px 0;
                        z-index: 1000;
                        min-width: 140px;
                    }
                    #context-menu.visible {
                        display: block;
                    }
                    .menu-item {
                        padding: 6px 14px;
                        font-size: 0.85rem;
                        cursor: pointer;
                    }
                    .menu-item:hover {
                        background-color: #F0F0F0;
                    }

                    @media (prefers-color-scheme: dark) {
                        body { color: #FFFFFF; }
                        main {
                            border: 1px solid #5C6BC0;
                            background-color: #1E1E1E;
                        }
                        #context-menu {
                            background: #2D2D2D;
                            border-color: #424242;
                        }
                        .menu-item:hover { background-color: #3A3A3A; }
                    }
                </style>
                <script>
                    function updateInputMethod(text) {
                        document.querySelector('main').innerText = text;
                    }

                    document.addEventListener('DOMContentLoaded', () => {
                        document.querySelector('main').addEventListener('click', (e) => {
                            e.stopPropagation();
                            window.ipc.postMessage(JSON.stringify({ type: 'toggle_input_mode' }));
                        });
                    });

                    document.addEventListener('contextmenu', (e) => {
                        e.preventDefault();
                        const menu = document.getElementById('context-menu');
                        menu.style.left = e.clientX + 'px';
                        menu.style.top = e.clientY + 'px';
                        menu.classList.add('visible');
                    });

                    document.addEventListener('click', (e) => {
                        if (!e.target.closest('#context-menu')) {
                            document.getElementById('context-menu').classList.remove('visible');
                        }
                    });

                    function openSettings() {
                        window.ipc.postMessage(JSON.stringify({ type: 'open_settings' }));
                    }

                    function toggleLearning() {
                        window.ipc.postMessage(JSON.stringify({ type: 'toggle_learning' }));
                    }
                </script>
            </head>
            <body style="margin: 0;">
                <main>
                    あ
                </main>
                <div id="context-menu">
                    <div class="menu-item" onclick="openSettings()">設定を開く</div>
                    <div class="menu-item" onclick="toggleLearning()">学習 ON/OFF</div>
                </div>
            </body>
        </html>"##,
        )
        .with_ipc_handler(|message| {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(message.body()) {
                match msg.get("type").and_then(|v| v.as_str()) {
                    Some("toggle_input_mode") => unsafe {
                        // Send VK_DBE_SBCSCHAR (0xF3) key press to toggle IME input mode
                        let inputs = [
                            INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0xF3),
                                        wScan: 0,
                                        dwFlags: KEYBD_EVENT_FLAGS(0),
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            },
                            INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0xF3),
                                        wScan: 0,
                                        dwFlags: KEYBD_EVENT_FLAGS(2), // KEYEVENTF_KEYUP
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            },
                        ];
                        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                    },
                    Some("open_settings") => {
                        let _ = std::process::Command::new(
                            std::env::current_exe()
                                .unwrap_or_default()
                                .parent()
                                .map(|p| p.join("azookey_settings.exe"))
                                .unwrap_or_else(|| std::path::PathBuf::from("azookey_settings.exe")),
                        )
                        .spawn();
                    }
                    Some("toggle_learning") => {
                        let mut config = shared::AppConfig::read();
                        config.learning.enable = !config.learning.enable;
                        config.write();
                    }
                    _ => {}
                }
            }
        })
        .build(&window)
        .context("Failed to create webview")?;

    Ok(webview)
}
