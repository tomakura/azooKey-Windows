use anyhow::{Context as _, Result};
use tao::{
    event_loop::EventLoop,
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        SetWindowLongW, GWL_EXSTYLE, GWL_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_POPUP,
    },
};
use wry::WebViewBuilder;

use crate::UserEvent;

pub fn create_candidate_window(event_loop: &EventLoop<UserEvent>) -> Result<Window> {
    let window = WindowBuilder::new()
        .with_decorations(false)
        .with_title("CandidateList")
        .with_focused(false)
        .with_visible(false)
        .with_undecorated_shadow(false)
        .with_transparent(true)
        .build(event_loop)
        .context("Failed to create window")?;

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

fn build_candidate_html() -> String {
    let config = shared::AppConfig::read();
    let appearance = &config.appearance;

    let theme_style = if appearance.custom_css_enabled {
        format!("<style>{}</style>", appearance.custom_css)
    } else {
        let is_custom = appearance.background_color != "#FFFFFF"
            || appearance.accent_color != "#2CB5FF"
            || appearance.text_color != "#000000";
        if is_custom {
            let bg = &appearance.background_color;
            let accent = &appearance.accent_color;
            let fg = &appearance.text_color;
            let sel_bg = format!("{accent}33");
            format!(
                "<style>:root{{--bg:{bg};--accent:{accent};--fg:{fg};--sel-bg:{sel_bg};}}\
                 body{{color:var(--fg);}}\
                 main{{background-color:var(--bg);border-color:color-mix(in srgb,var(--fg) 15%,transparent);}}\
                 li[data-selected]{{background-color:var(--sel-bg);outline-color:var(--accent);}}</style>"
            )
        } else {
            String::new()
        }
    };

    [CANDIDATE_HTML_HEAD, theme_style.as_str(), CANDIDATE_HTML_TAIL].concat()
}

const CANDIDATE_HTML_HEAD: &str = r##"
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
                        padding: 8px;
                        border: 1px solid #E4E4E4;
                        border-radius: 10px;
                        background-color: #FFFFFF;
                        box-sizing: border-box;
                        display: flex;
                        flex-direction: column;
                    }
                    ol {
                        margin: 0;
                        padding: 0;
                        flex: 1;
                        overflow-y: auto;
                        scroll-snap-type: y proximity;
                        list-style-position: inside;
                        list-style-type: none;
                        user-select: none;
                        cursor: pointer;

                        &::-webkit-scrollbar {
                            width: 5px;
                        }

                        &::-webkit-scrollbar-thumb {
                            background-color: #BCBCBC;
                            border-radius: 10px;
                        }
                    }
                    li {
                        padding: 0.5rem;
                        font-size: 0.9rem;
                        display: flex;
                        align-items: center;
                        gap: 0.5rem;
                        scroll-snap-align: start;

                        &::before {
                            content: attr(data-shortcut);
                            color: #636363;
                            font-weight: bold;
                            font-size: 0.75rem;
                            margin: 0 0.75rem 0 2;
                            width: 0.75rem;
                        }

                        &[data-selected] {
                            background-color: #D4F0FF;
                            border-radius: 3px;
                            margin-right: 5px;
                            outline: 1px solid #2CB5FF;
                            outline-offset: -1px;
                        }
                    }
                    .candidate-text {
                        flex: 1;
                        min-width: 0;
                        overflow: hidden;
                        text-overflow: ellipsis;
                        white-space: nowrap;
                    }
                    .candidate-subtext {
                        color: #757575;
                        font-size: 0.78rem;
                        min-width: 0;
                        overflow: hidden;
                        text-overflow: ellipsis;
                        white-space: nowrap;
                    }
                    footer {
                        display: flex;
                        justify-content: space-between;
                        align-items: center;
                        padding: 8 10 5 10;
                        border-top: 1px solid #E4E4E4;
                        font-size: 0.8rem;
                        user-select: none;
                    }

                    @media (prefers-color-scheme: dark) {
                        body {
                            color: #FFFFFF;
                        }
                        main {
                            border: 1px solid #424242;
                            background-color: #1E1E1E;
                        }
                        ol::-webkit-scrollbar-thumb {
                            background-color: #757575;
                        }
                        li {
                            color: #E0E0E0;

                            &::before {
                                color: #BDBDBD;
                            }

                            &[data-selected] {
                                background-color: #3949AB;
                                outline: 1px solid #5C6BC0;
                            }
                        }
                        .candidate-subtext {
                            color: #BDBDBD;
                        }

                        footer {
                            border-top: 1px solid #424242;
                        }
                    }
                </style>"##;

const CANDIDATE_HTML_TAIL: &str = r##"
                <script>
                    function updateCandidates(candidates) {
                        const candidateList = document.getElementById('candidate-list');

                        const existingItems = Array.from(candidateList.children);

                        candidates.forEach((candidate, index) => {
                            const item = typeof candidate === 'string' ? { text: candidate, subtext: '' } : candidate;
                            const shortcut = index < 9 ? String(index + 1) : (index === 9 ? '0' : '');
                            if (existingItems[index]) {
                                existingItems[index].dataset.shortcut = shortcut;
                                existingItems[index].querySelector('.candidate-text').textContent = item.text;
                                existingItems[index].querySelector('.candidate-subtext').textContent = item.subtext || '';
                            } else {
                                const li = document.createElement('li');
                                li.dataset.shortcut = shortcut;
                                const text = document.createElement('span');
                                text.className = 'candidate-text';
                                text.textContent = item.text;
                                const subtext = document.createElement('span');
                                subtext.className = 'candidate-subtext';
                                subtext.textContent = item.subtext || '';
                                li.appendChild(text);
                                li.appendChild(subtext);
                                candidateList.appendChild(li);
                            }
                        });

                        while (existingItems.length > candidates.length) {
                            candidateList.removeChild(existingItems.pop());
                        }
                    }

                    function updateSelection(index) {
                        const candidateList = document.getElementById('candidate-list');
                        if (!candidateList.children.length || !candidateList.children[index]) {
                            return;
                        }
                        const selected = candidateList.querySelector('[data-selected]');
                        if (selected) {
                            selected.removeAttribute('data-selected');
                        }

                        candidateList.children[index].setAttribute('data-selected', '');

                        const itemHeight = candidateList.children[0].offsetHeight;

                        const groupSize = 5;
                        const groupIndex = Math.floor(index / groupSize);
                        const scrollToIndex = groupIndex * groupSize;

                        if (index === scrollToIndex || !isElementInView(candidateList.children[index], candidateList)) {
                            candidateList.children[scrollToIndex].scrollIntoView({ behavior: "instant", block: "start", inline: "start" });
                        }
                    }

                    function isElementInView(element, container) {
                        const containerRect = container.getBoundingClientRect();
                        const elementRect = element.getBoundingClientRect();

                        return (
                            elementRect.top >= containerRect.top &&
                            elementRect.bottom <= containerRect.bottom
                        );
                    }

                    function adjustWindowSize() {
                        const candidateList = document.getElementById('candidate-list');

                        candidateList.innerHTML = '';

                        for (let i = 0; i < 5; i++) {
                            const li = document.createElement('li');
                            li.textContent = `Item ${i+1}`;
                            candidateList.appendChild(li);
                        }

                        const footer = document.querySelector('footer');
                        const main = document.querySelector('main');
                        const body = document.body;

                        const itemHeight = candidateList.children[0].offsetHeight;
                        const candidateListHeight = itemHeight * 5;
                        const footerHeight = footer.offsetHeight;
                        const mainPadding = parseInt(window.getComputedStyle(main).paddingTop) +
                                           parseInt(window.getComputedStyle(main).paddingBottom);
                        const bodyPadding = parseInt(window.getComputedStyle(body).paddingTop) +
                                          parseInt(window.getComputedStyle(body).paddingBottom);

                        const totalHeight = candidateListHeight + footerHeight + mainPadding + bodyPadding;

                        candidateList.innerHTML = '';

                        window.ipc.postMessage(JSON.stringify({
                            type: 'resize',
                            height: totalHeight
                        }));
                    }

                    window.addEventListener('DOMContentLoaded', () => {
                        setTimeout(adjustWindowSize, 50);
                    });
                </script>
            </head>
            <body style="margin: 0;">
                <main>
                    <ol id="candidate-list">
                    </ol>
                    <footer>
                        <svg width="20" height="14" viewBox="0 0 22 16" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M3.5 8C4.59202 9.04403 7.54398 10.3978 13.5068 9.93754M1.25349 5.39919C2.77722 0.413397 8.08911 0.79692 10.9673 1.24436C14.2687 1.71311 20.8969 3.82675 20.9985 8.53129C21.1255 14.412 13.1894 15.3069 10.0784 14.9233C6.96748 14.5398 -0.46071 13.0696 1.25349 5.39919Z" stroke="#838384" stroke-width="1.5" stroke-linecap="round"/>
                        </svg>
                    </footer>
                </main>
            </body>
        </html>"##;

pub fn create_candidate_webview<'a>() -> Result<WebViewBuilder<'a>> {
    let html = build_candidate_html();
    let webview_builder = WebViewBuilder::new()
        .with_transparent(true)
        .with_html(html);
    Ok(webview_builder)
}
