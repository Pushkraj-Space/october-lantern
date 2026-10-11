//! `lantern-engine attach <name>`: shows a Lantern session in this console window, the way
//! `tmux attach` does. Closing the window leaves the agent running.
use super::*;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use windows_sys::Win32::System::Console::{
    CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCP, SetConsoleMode, SetConsoleOutputCP,
};
use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

/// Opens a terminal window attached to the session: a Windows Terminal tab, or a console
/// window where Windows Terminal isn't installed.
pub fn open_window(name: &str) -> Result<()> {
    if !valid_name(name) {
        bail!("invalid session name");
    }
    let engine = crate::hooks::engine_copy()?;
    // The new window may come to the front (Lantern was just clicked).
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let quiet = |cmd: &mut Command| {
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    };
    if let Some(wt) = crate::platform::find_on_path("wt") {
        let mut cmd = Command::new(wt);
        cmd.args(["-w", "0", "new-tab", "--title", name, "--"]).arg(&engine).args(["attach", name]);
        quiet(&mut cmd);
        if let Ok(mut child) = cmd.spawn() {
            std::thread::spawn(move || child.wait());
            return Ok(());
        }
    }
    let mut cmd = Command::new(&engine);
    cmd.args(["attach", name]).creation_flags(CREATE_NEW_CONSOLE);
    quiet(&mut cmd);
    let mut child = cmd.spawn().context("opening a console window")?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Console modes to put back when the session ends.
struct Modes {
    input: CONSOLE_MODE,
    output: CONSOLE_MODE,
}

fn console_size() -> Option<(u16, u16)> {
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return None;
    }
    let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(1) as u16;
    let rows = (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as u16;
    Some((cols, rows))
}

/// Keys go to the agent as typed (arrows as escape sequences, Ctrl+C as a character), and its
/// output is drawn as the terminal escape sequences it is.
fn raw_console() -> Option<Modes> {
    unsafe {
        let (input, output) = (GetStdHandle(STD_INPUT_HANDLE), GetStdHandle(STD_OUTPUT_HANDLE));
        let (mut in_mode, mut out_mode) = (0, 0);
        if GetConsoleMode(input, &mut in_mode) == 0 || GetConsoleMode(output, &mut out_mode) == 0 {
            return None;
        }
        SetConsoleMode(
            input,
            (in_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT)) | ENABLE_VIRTUAL_TERMINAL_INPUT,
        );
        SetConsoleMode(output, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN);
        SetConsoleCP(65001);
        SetConsoleOutputCP(65001);
        Some(Modes { input: in_mode, output: out_mode })
    }
}

fn restore(modes: &Option<Modes>) {
    if let Some(m) = modes {
        unsafe {
            SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), m.input);
            SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), m.output);
        }
    }
}

pub fn attach(name: &str) -> Result<()> {
    let modes = raw_console();
    let result = run(name);
    restore(&modes);
    let mut out = std::io::stdout();
    let _ = write!(out, "\r\n[Lantern] {}\r\n", if result.is_ok() { "The session has ended." } else { "Couldn't show the session." });
    let _ = out.flush();
    if let Err(e) = &result {
        eprintln!("{e:#}");
    }
    std::thread::sleep(Duration::from_secs(2));
    result
}

fn run(name: &str) -> Result<()> {
    // Size the session to this window before drawing it.
    let mut size = console_size();
    if let Some((cols, rows)) = size {
        let _ = request(name, &json!({"op": "resize", "cols": cols, "rows": rows}));
    }
    let output = connect(name)?;
    writeln!(&output, "{}", json!({"op": "output"}))?;
    let mut output = BufReader::new(output);
    let mut line = String::new();
    output.read_line(&mut line)?;
    anyhow::ensure!(serde_json::from_str::<Value>(&line).is_ok_and(|v| v["ok"] == true), "the session refused to show");

    let input = connect(name)?;
    writeln!(&input, "{}", json!({"op": "input"}))?;
    let mut answer = String::new();
    BufReader::new(&input).read_line(&mut answer)?;
    std::thread::spawn(move || {
        let mut keys = [0u8; 4096];
        let mut stdin = std::io::stdin();
        while let Ok(n) = stdin.read(&mut keys) {
            if n == 0 || (&input).write_all(&keys[..n]).is_err() {
                break;
            }
        }
    });
    {
        let name = name.to_string();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(300));
                let now = console_size();
                if now != size {
                    size = now;
                    if let Some((cols, rows)) = now
                        && request(&name, &json!({"op": "resize", "cols": cols, "rows": rows})).is_err()
                    {
                        return;
                    }
                }
            }
        });
    }

    let mut stdout = std::io::stdout();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let n = output.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        stdout.write_all(&chunk[..n])?;
        stdout.flush()?;
    }
}
