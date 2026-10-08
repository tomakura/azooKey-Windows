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
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, VIRTUAL_KEY,
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
                        position: relative;
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
                    #settings-button {
                        position: absolute;
                        right: 2px;
                        bottom: 2px;
                        width: 24px;
                        height: 24px;
                        border: 0;
                        border-radius: 4px;
                        background: transparent;
                        color: inherit;
                        cursor: pointer;
                        font-size: 16px;
                    }
                    #settings-button:hover { background: rgba(128,128,128,0.2); }
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
                        position: relative;
                            border: 1px solid #5C6BC0;
                            background-color: #1E1E1E;
                        }
                        #settings-button {
                        position: absolute;
                        right: 2px;
                        bottom: 2px;
                        width: 24px;
                        height: 24px;
                        border: 0;
                        border-radius: 4px;
                        background: transparent;
                        color: inherit;
                        cursor: pointer;
                        font-size: 16px;
                    }
                    #settings-button:hover { background: rgba(128,128,128,0.2); }
                    #context-menu {
                            background: #2D2D2D;
                            border-color: #424242;
                        }
                        .menu-item:hover { background-color: #3A3A3A; }
                    }
                </style>
                <script>
                    function updateTheme(css) {
                        let style = document.getElementById('user-theme');
                        if (!style) {
                            style = document.createElement('style');
                            style.id = 'user-theme';
                            document.head.appendChild(style);
                        }
                        style.textContent = css;
                    }
                    function updateInputMethod(text) {
                        document.getElementById('input-mode').innerText = text;
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
                    <span id="input-mode">あ</span>
                    <button id="settings-button" title="設定を開く" aria-label="設定を開く" onclick="event.stopPropagation(); openSettings()">⚙</button>
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
                        let result = (|| -> std::io::Result<()> {
                            let exe = std::env::current_exe()?;
                            let directory = exe.parent().ok_or_else(|| std::io::Error::other("UI directory is missing"))?;
                            std::process::Command::new(directory.join("azookey_settings.exe")).spawn()?;
                            Ok(())
                        })();
                        if let Err(error) = result {
                            let message = windows::core::HSTRING::from(format!("設定画面を開けませんでした: {error}"));
                            unsafe {
                                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                                    None, &message, windows::core::w!("Azookey"),
                                    windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                                );
                            }
                        }
                    }
                    Some("toggle_learning") => {
                        tokio::spawn(async {
                            let previous = shared::AppConfig::read();
                            let mut config = previous.clone();
                            config.learning.enable = !config.learning.enable;
                            let result = async {
                                config.try_write()?;
                                crate::ipc::update_server_config().await
                            }
                            .await;
                            if let Err(error) = result {
                                let restore = previous.try_write();
                                let message = windows::core::HSTRING::from(format!(
                                    "学習設定を更新できませんでした: {error}\n設定の復元: {restore:?}"
                                ));
                                unsafe {
                                    windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                                        None,
                                        &message,
                                        windows::core::w!("Azookey"),
                                        windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                                    );
                                }
                            }
                        });
                    }
                    _ => {}
                }
            }
        })
        .build(&window)
        .context("Failed to create webview")?;

    Ok(webview)
}
