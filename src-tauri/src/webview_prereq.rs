#[cfg(windows)]
use std::{
    env,
    ffi::c_void,
    fs, mem,
    path::{Path, PathBuf},
    process::Command,
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

#[cfg(windows)]
use futures::StreamExt;
#[cfg(windows)]
use tauri::api::dialog::blocking::MessageDialogBuilder;
#[cfg(windows)]
use tauri::api::dialog::{MessageDialogButtons, MessageDialogKind};
#[cfg(windows)]
use tokio::io::AsyncWriteExt;
#[cfg(windows)]
use windows_sys::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{GetStockObject, UpdateWindow, COLOR_WINDOW, DEFAULT_GUI_FONT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                InitCommonControlsEx, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, PBM_SETMARQUEE,
                PBM_SETPOS, PBM_SETRANGE32, PBS_MARQUEE, PBS_SMOOTH, PROGRESS_CLASSW,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
                LoadCursorW, PeekMessageW, PostQuitMessage, RegisterClassW, SendMessageW,
                SetWindowTextW, ShowWindow, TranslateMessage, IDC_ARROW, MSG, PM_REMOVE,
                SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_CLOSE, WM_DESTROY, WM_QUIT, WM_SETFONT,
                WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPED,
                WS_VISIBLE,
            },
        },
    },
};
#[cfg(windows)]
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
#[cfg(windows)]
use winreg::{RegKey, HKEY};

#[cfg(windows)]
use crate::process_util::hide_window;

#[cfg(windows)]
const WEBVIEW2_DOWNLOAD_URL_ENV: &str = "SKILL_HUB_WEBVIEW2_INSTALLER_URL";
#[cfg(windows)]
const WEBVIEW2_INSTALL_ARGS_ENV: &str = "SKILL_HUB_WEBVIEW2_INSTALLER_ARGS";
#[cfg(windows)]
const FORCE_WEBVIEW2_SETUP_ENV: &str = "SKILL_HUB_FORCE_WEBVIEW2_SETUP";
#[cfg(windows)]
const WEBVIEW2_DOWNLOAD_URL_FILE: &str = "webview2-installer-url.txt";
#[cfg(windows)]
const DEFAULT_WEBVIEW2_INSTALLER_URL: &str = match option_env!("SKILL_HUB_WEBVIEW2_INSTALLER_URL") {
    Some(value) => value,
    None => "http://intranet.example.com/MicrosoftEdgeWebView2RuntimeInstallerX64.exe",
};
#[cfg(windows)]
const DEFAULT_WEBVIEW2_INSTALLER_ARGS: &str = match option_env!("SKILL_HUB_WEBVIEW2_INSTALLER_ARGS")
{
    Some(value) => value,
    None => "/silent /install",
};
#[cfg(windows)]
const WEBVIEW2_RUNTIME_CLIENT_ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
#[cfg(windows)]
const STATIC_CLASS: PCWSTR = w!("STATIC");
#[cfg(windows)]
const PROGRESS_CLASS_NAME: PCWSTR = w!("SkillHubWebView2SetupWindow");

#[cfg(windows)]
pub fn ensure_webview2_runtime_or_exit() {
    if is_webview2_runtime_available() && !force_webview2_setup() {
        return;
    }

    let installer_url = webview2_installer_url();
    let install_args = webview2_installer_args();
    let installer_path = temp_installer_path(&installer_url);
    let confirmed = MessageDialogBuilder::new(
        "需要安装 WebView2 Runtime",
        format!(
            "当前电脑未检测到 Microsoft Edge WebView2 Runtime，Skill Hub 需要该组件才能显示应用窗口。\n\n\
             应用将从内网地址下载安装包到临时目录，然后启动安装程序：\n{installer_url}\n\n\
             临时目录：\n{}\n\n\
             安装完成后会尝试重新打开 Skill Hub；如果安装器要求管理员权限，请按系统提示允许。",
            installer_path.display()
        ),
    )
    .kind(MessageDialogKind::Warning)
    .buttons(MessageDialogButtons::OkCancelWithLabels(
        "下载并安装".to_string(),
        "退出".to_string(),
    ))
    .show();

    if !confirmed {
        std::process::exit(2);
    }

    let progress = SetupProgress::open();
    progress.set_status(
        "正在准备 WebView2 Runtime",
        "连接内网下载地址，准备获取安装包。",
    );

    if let Err(error) = download_installer_to_temp(&installer_url, &installer_path, &progress) {
        progress.fail("下载失败", "未能获取 WebView2 安装包。");
        MessageDialogBuilder::new(
            "WebView2 下载失败",
            format!(
                "未能从内网地址下载安装包：\n{installer_url}\n\n错误：{error}\n\n\
                 请检查网络，或手动下载安装 MicrosoftEdgeWebView2RuntimeInstallerX64.exe。"
            ),
        )
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show();
        std::process::exit(2);
    }

    progress.start_installing(&format!(
        "安装包已保存到：{}。正在启动安装程序，请按系统提示完成授权。",
        installer_path.display()
    ));

    if let Err(error) = run_installer(&installer_path, &install_args, &progress) {
        progress.fail("安装程序启动失败", "请手动运行临时目录中的安装包。");
        MessageDialogBuilder::new(
            "WebView2 安装程序启动失败",
            format!(
                "安装包已下载，但未能启动安装程序：\n{}\n\n错误：{error}\n\n\
                 请手动运行该安装包，安装完成后重新打开 Skill Hub。",
                installer_path.display()
            ),
        )
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show();
        std::process::exit(2);
    }

    progress.set_status(
        "正在检测安装结果",
        "安装程序已结束，正在确认 WebView2 Runtime 是否可用。",
    );

    if wait_for_webview2_runtime(Duration::from_secs(30), &progress) {
        progress.complete(
            "安装完成",
            "WebView2 Runtime 已可用，准备重新打开 Skill Hub。",
        );
        let relaunch = MessageDialogBuilder::new(
            "WebView2 Runtime 已安装",
            "安装完成。点击确定后将重新打开 Skill Hub；如果没有自动打开，请手动重新启动应用。",
        )
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok)
        .show();

        progress.close();
        if relaunch && relaunch_app() {
            std::process::exit(0);
        }
        return;
    }

    progress.fail(
        "仍未检测到 WebView2",
        "安装可能尚未完成，请重新打开应用再试。",
    );
    MessageDialogBuilder::new(
        "请完成 WebView2 安装",
        format!(
            "安装程序已运行，但当前仍未检测到 WebView2 Runtime。\n\n\
             请确认安装过程是否完成，必要时手动运行安装包：\n{}\n\n\
             安装完成后请重新打开 Skill Hub。",
            installer_path.display()
        ),
    )
    .kind(MessageDialogKind::Error)
    .buttons(MessageDialogButtons::Ok)
    .show();
    std::process::exit(2);
}

#[cfg(not(windows))]
pub fn ensure_webview2_runtime_or_exit() {}

#[cfg(windows)]
fn webview2_installer_url() -> String {
    env::var(WEBVIEW2_DOWNLOAD_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(installer_url_from_sidecar_file)
        .unwrap_or_else(|| DEFAULT_WEBVIEW2_INSTALLER_URL.to_string())
}

#[cfg(windows)]
fn force_webview2_setup() -> bool {
    env::var(FORCE_WEBVIEW2_SETUP_ENV)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn webview2_installer_args() -> Vec<String> {
    env::var(WEBVIEW2_INSTALL_ARGS_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_WEBVIEW2_INSTALLER_ARGS.to_string())
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

#[cfg(windows)]
fn installer_url_from_sidecar_file() -> Option<String> {
    let exe_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    let content = fs::read_to_string(exe_dir.join(WEBVIEW2_DOWNLOAD_URL_FILE)).ok()?;
    let value = content.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(windows)]
fn temp_installer_path(installer_url: &str) -> PathBuf {
    let file_name = installer_url
        .split('?')
        .next()
        .unwrap_or(installer_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(safe_file_name)
        .filter(|name| name.to_ascii_lowercase().ends_with(".exe"))
        .unwrap_or_else(|| "MicrosoftEdgeWebView2RuntimeInstallerX64.exe".to_string());

    env::temp_dir().join("SkillHub").join(file_name)
}

#[cfg(windows)]
fn is_webview2_runtime_available() -> bool {
    webview2_runtime_version().is_some()
}

#[cfg(windows)]
fn webview2_runtime_version() -> Option<String> {
    [
        version_from_root(HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        version_from_root(HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        version_from_root(HKEY_CURRENT_USER, KEY_WOW64_64KEY),
        version_from_root(HKEY_CURRENT_USER, KEY_WOW64_32KEY),
    ]
    .into_iter()
    .flatten()
    .find(|version| !version.trim().is_empty())
}

#[cfg(windows)]
fn version_from_root(root: HKEY, wow64_flag: u32) -> Option<String> {
    let clients_path = format!(
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{}",
        WEBVIEW2_RUNTIME_CLIENT_ID
    );
    let key = RegKey::predef(root)
        .open_subkey_with_flags(clients_path, KEY_READ | wow64_flag)
        .ok()?;
    key.get_value::<String, _>("pv")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(windows)]
fn safe_file_name(value: &str) -> String {
    let name = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        .collect::<String>();
    if name.is_empty() {
        "MicrosoftEdgeWebView2RuntimeInstallerX64.exe".to_string()
    } else {
        name
    }
}

#[cfg(windows)]
fn download_installer_to_temp(
    url: &str,
    destination: &Path,
    progress: &SetupProgress,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("创建下载运行时失败：{error}"))?;
    runtime.block_on(download_installer_to_temp_async(
        url,
        destination,
        progress.clone(),
    ))
}

#[cfg(windows)]
async fn download_installer_to_temp_async(
    url: &str,
    destination: &Path,
    progress: SetupProgress,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定临时目录".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("创建临时目录失败：{error}"))?;

    progress.set_status("正在下载 WebView2 安装包", "正在连接内网服务器。");
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| format!("请求安装包失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("安装包下载失败，HTTP 状态码：{status}"));
    }

    let total_size = response.content_length();
    match total_size {
        Some(total) if total > 0 => progress.set_download_progress(0, Some(total)),
        _ => progress.start_download_indeterminate("服务器未返回文件大小，正在下载。"),
    }

    let temp_path = destination.with_extension("download");
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|error| format!("创建临时安装包失败：{error}"))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("读取下载数据失败：{error}"))?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("写入安装包失败：{error}"))?;

        progress.set_download_progress(downloaded, total_size);
    }

    file.flush()
        .await
        .map_err(|error| format!("保存安装包失败：{error}"))?;
    drop(file);

    if downloaded == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err("下载内容为空".to_string());
    }

    if tokio::fs::try_exists(destination).await.unwrap_or(false) {
        tokio::fs::remove_file(destination)
            .await
            .map_err(|error| format!("替换旧安装包失败：{error}"))?;
    }
    tokio::fs::rename(&temp_path, destination)
        .await
        .map_err(|error| format!("保存安装包到临时目录失败：{error}"))?;
    progress.set_download_progress(downloaded, Some(downloaded));
    Ok(())
}

#[cfg(windows)]
fn run_installer(
    installer_path: &Path,
    args: &[String],
    progress: &SetupProgress,
) -> Result<(), String> {
    let mut command = Command::new(installer_path);
    hide_window(&mut command).args(args);
    progress.start_installing("安装程序正在运行，系统可能会弹出权限确认窗口。");
    let status = command
        .status()
        .map_err(|error| format!("启动安装程序失败：{error}"))?;
    if status.success() || is_webview2_runtime_available() {
        Ok(())
    } else {
        Err(format!("安装程序退出码：{status}"))
    }
}

#[cfg(windows)]
fn wait_for_webview2_runtime(timeout: Duration, progress: &SetupProgress) -> bool {
    let attempts = (timeout.as_secs().max(1) * 2) as usize;
    for index in 0..attempts {
        if is_webview2_runtime_available() {
            return true;
        }
        progress.set_status(
            "正在检测安装结果",
            &format!(
                "等待 WebView2 Runtime 写入系统信息。第 {} 次检测。",
                index + 1
            ),
        );
        thread::sleep(Duration::from_millis(500));
    }
    false
}

#[cfg(windows)]
fn relaunch_app() -> bool {
    let Ok(exe) = env::current_exe() else {
        return false;
    };
    let mut command = Command::new(exe);
    hide_window(&mut command).args(env::args_os().skip(1));
    command.spawn().is_ok()
}

#[cfg(windows)]
#[derive(Clone)]
struct SetupProgress {
    sender: Option<Sender<ProgressCommand>>,
}

#[cfg(windows)]
impl SetupProgress {
    fn open() -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || progress_window_thread(receiver));
        Self {
            sender: Some(sender),
        }
    }

    fn set_status(&self, title: &str, detail: &str) {
        self.send(ProgressCommand::Status {
            title: title.to_string(),
            detail: detail.to_string(),
        });
    }

    fn set_download_progress(&self, downloaded: u64, total: Option<u64>) {
        let percent = total
            .filter(|total| *total > 0)
            .map(|total| ((downloaded.saturating_mul(100)) / total).min(100) as u32);
        let detail = match (percent, total) {
            (Some(percent), Some(total)) => format!(
                "已下载 {} / {}，{}%",
                human_size(downloaded),
                human_size(total),
                percent
            ),
            _ => format!("已下载 {}。", human_size(downloaded)),
        };
        self.send(ProgressCommand::Progress {
            title: "正在下载 WebView2 安装包".to_string(),
            detail,
            percent,
        });
    }

    fn start_download_indeterminate(&self, detail: &str) {
        self.send(ProgressCommand::Indeterminate {
            title: "正在下载 WebView2 安装包".to_string(),
            detail: detail.to_string(),
        });
    }

    fn start_installing(&self, detail: &str) {
        self.send(ProgressCommand::Indeterminate {
            title: "正在安装 WebView2 Runtime".to_string(),
            detail: detail.to_string(),
        });
    }

    fn complete(&self, title: &str, detail: &str) {
        self.send(ProgressCommand::Progress {
            title: title.to_string(),
            detail: detail.to_string(),
            percent: Some(100),
        });
    }

    fn fail(&self, title: &str, detail: &str) {
        self.send(ProgressCommand::Status {
            title: title.to_string(),
            detail: detail.to_string(),
        });
    }

    fn close(&self) {
        self.send(ProgressCommand::Close);
    }

    fn send(&self, command: ProgressCommand) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }
}

#[cfg(windows)]
enum ProgressCommand {
    Status {
        title: String,
        detail: String,
    },
    Progress {
        title: String,
        detail: String,
        percent: Option<u32>,
    },
    Indeterminate {
        title: String,
        detail: String,
    },
    Close,
}

#[cfg(windows)]
fn progress_window_thread(receiver: Receiver<ProgressCommand>) {
    let Some(mut window) = ProgressWindow::new() else {
        return;
    };
    window.set_indeterminate("正在准备 WebView2 Runtime", "安装引导正在启动，请稍候。");

    loop {
        while let Ok(command) = receiver.try_recv() {
            match command {
                ProgressCommand::Status { title, detail } => window.set_status(&title, &detail),
                ProgressCommand::Progress {
                    title,
                    detail,
                    percent,
                } => match percent {
                    Some(value) => window.set_progress(&title, &detail, value),
                    None => window.set_indeterminate(&title, &detail),
                },
                ProgressCommand::Indeterminate { title, detail } => {
                    window.set_indeterminate(&title, &detail)
                }
                ProgressCommand::Close => {
                    window.close();
                    return;
                }
            }
        }

        if !window.pump_message() {
            return;
        }
        thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(windows)]
struct ProgressWindow {
    hwnd: HWND,
    title_label: HWND,
    detail_label: HWND,
    progress_bar: HWND,
}

#[cfg(windows)]
impl ProgressWindow {
    fn new() -> Option<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());
            if hinstance == 0 {
                return None;
            }

            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(progress_window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: 0,
                hCursor: LoadCursorW(0, IDC_ARROW),
                hbrBackground: (COLOR_WINDOW + 1) as _,
                lpszMenuName: ptr::null(),
                lpszClassName: PROGRESS_CLASS_NAME,
            };
            RegisterClassW(&class);

            let window_width = 520;
            let window_height = 190;
            let x = (GetSystemMetrics(SM_CXSCREEN) - window_width) / 2;
            let y = (GetSystemMetrics(SM_CYSCREEN) - window_height) / 2;
            let hwnd = CreateWindowExW(
                0,
                PROGRESS_CLASS_NAME,
                w!("Skill Hub 组件安装"),
                WS_OVERLAPPED | WS_CAPTION | WS_CLIPCHILDREN,
                x,
                y,
                window_width,
                window_height,
                0,
                0,
                hinstance,
                ptr::null::<c_void>(),
            );
            if hwnd == 0 {
                return None;
            }

            let init_controls = INITCOMMONCONTROLSEX {
                dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_PROGRESS_CLASS,
            };
            InitCommonControlsEx(&init_controls);

            let title_label = CreateWindowExW(
                0,
                STATIC_CLASS,
                w!("正在准备 WebView2 Runtime"),
                WS_CHILD | WS_VISIBLE,
                24,
                24,
                460,
                26,
                hwnd,
                0,
                hinstance,
                ptr::null::<c_void>(),
            );
            let detail_label = CreateWindowExW(
                0,
                STATIC_CLASS,
                w!("安装引导正在启动，请稍候。"),
                WS_CHILD | WS_VISIBLE,
                24,
                58,
                460,
                42,
                hwnd,
                0,
                hinstance,
                ptr::null::<c_void>(),
            );
            let progress_bar = CreateWindowExW(
                0,
                PROGRESS_CLASSW,
                ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | PBS_SMOOTH | PBS_MARQUEE,
                24,
                116,
                460,
                18,
                hwnd,
                0,
                hinstance,
                ptr::null::<c_void>(),
            );

            if title_label == 0 || detail_label == 0 || progress_bar == 0 {
                DestroyWindow(hwnd);
                return None;
            }

            let font = GetStockObject(DEFAULT_GUI_FONT);
            if font != 0 {
                SendMessageW(title_label, WM_SETFONT, font as WPARAM, 1);
                SendMessageW(detail_label, WM_SETFONT, font as WPARAM, 1);
                SendMessageW(progress_bar, WM_SETFONT, font as WPARAM, 1);
            }

            SendMessageW(progress_bar, PBM_SETRANGE32, 0, 100);
            SendMessageW(progress_bar, PBM_SETPOS, 0, 0);
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);

            Some(Self {
                hwnd,
                title_label,
                detail_label,
                progress_bar,
            })
        }
    }

    fn set_status(&mut self, title: &str, detail: &str) {
        self.set_text(title, detail);
        unsafe {
            SendMessageW(self.progress_bar, PBM_SETMARQUEE, 0, 0);
        }
    }

    fn set_progress(&mut self, title: &str, detail: &str, percent: u32) {
        self.set_text(title, detail);
        unsafe {
            SendMessageW(self.progress_bar, PBM_SETMARQUEE, 0, 0);
            SendMessageW(self.progress_bar, PBM_SETPOS, percent.min(100) as WPARAM, 0);
        }
    }

    fn set_indeterminate(&mut self, title: &str, detail: &str) {
        self.set_text(title, detail);
        unsafe {
            SendMessageW(self.progress_bar, PBM_SETMARQUEE, 1, 35);
        }
    }

    fn set_text(&mut self, title: &str, detail: &str) {
        let title = truncate_for_native_label(title, 64);
        let detail = truncate_for_native_label(detail, 128);
        let title_wide = wide_null(&title);
        let detail_wide = wide_null(&detail);
        unsafe {
            SetWindowTextW(self.title_label, title_wide.as_ptr());
            SetWindowTextW(self.detail_label, detail_wide.as_ptr());
            UpdateWindow(self.hwnd);
        }
    }

    fn pump_message(&mut self) -> bool {
        unsafe {
            let mut message = MSG {
                hwnd: 0,
                message: 0,
                wParam: 0,
                lParam: 0,
                time: 0,
                pt: POINT { x: 0, y: 0 },
            };
            while PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) > 0 {
                if message.message == WM_QUIT {
                    return false;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            true
        }
    }

    fn close(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn progress_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLOSE => {
            return 0;
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
fn truncate_for_native_label(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

#[cfg(windows)]
fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}
