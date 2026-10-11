//! Typing a reply into an agent's own Windows console: Windows Terminal, VS Code's terminal or a
//! console window, for agents Lantern didn't start (its own sessions go through pty.rs).
//!
//! Attaching to a console changes the whole process, so the engine never does it itself: each
//! delivery runs `lantern-engine console-type`, which attaches to the agent's console, checks it,
//! writes the keystrokes into the console's input and exits.
//!
//! Windows can't say which process reads a console's input, so instead of the foreground check
//! used on macOS and Linux the helper refuses unless:
//! - the agent is the process Lantern saw (same pid and start time), still alive,
//! - it is still attached to the console (so the text can't land in a shell after it exits),
//! - nothing else is attached except the agent and the programs that started it (its shell). A
//!   tool the agent is running in that terminal would be attached too, and could read the text.
use anyhow::Result;

/// One key press: its virtual key, its scan code and the UTF-16 unit it types.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub vk: u16,
    pub scan: u16,
    pub unit: u16,
}

#[cfg(windows)]
const ENTER: Key = Key { vk: 0x0D, scan: 0x1C, unit: b'\r' as u16 };
#[cfg(any(windows, test))]
const ESCAPE: Key = Key { vk: 0x1B, scan: 0x01, unit: 0x1B };

/// What a delivery types: one line of text, or a single key ("escape" or one character).
#[cfg(any(windows, test))]
pub fn keys(mode: &str, payload: &str) -> Option<Vec<Key>> {
    let typed = |text: &str| text.encode_utf16().map(|unit| Key { vk: 0, scan: 0, unit }).collect::<Vec<_>>();
    match mode {
        "text" => Some(typed(&payload.replace(['\r', '\n'], " "))),
        "key" if payload == "escape" => Some(vec![ESCAPE]),
        "key" if payload.chars().count() == 1 => Some(typed(payload)),
        _ => None,
    }
}

/// Every process on the console must be the helper, the agent, one of the agent's ancestors, or
/// the agent's own native program under it (`native`: Codex's npm wrapper runs `codex.exe`).
#[cfg(any(windows, test))]
pub fn others_on_console(attached: &[u32], helper: u32, agent: u32, ancestors: &[u32], native: impl Fn(u32) -> bool) -> Vec<u32> {
    attached.iter().copied().filter(|pid| *pid != helper && *pid != agent && !ancestors.contains(pid) && !native(*pid)).collect()
}

/// The program an agent of this kind runs natively under a wrapper ("" for none).
pub fn native_program(kind: crate::model::Kind) -> &'static str {
    match kind {
        crate::model::Kind::Codex => "codex",
        _ => "",
    }
}

/// Engine side: types into the agent's console through a helper process (see the module comment).
pub fn send(agent: &crate::model::Agent, mode: &str, payload: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let (pid, start) = (agent.pid.to_string(), agent.start_time.to_string());
    let out = crate::run::output(
        std::process::Command::new(exe).args(["console-type", &pid, &start, native_program(agent.kind), mode, payload]),
        std::time::Duration::from_secs(5),
    )?;
    anyhow::ensure!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr).trim());
    Ok(())
}

/// The helper: `lantern-engine console-type <pid> <start> <native program> text|key <payload>`.
#[cfg(windows)]
pub fn type_into(pid: u32, start: u64, program: &str, mode: &str, payload: &str) -> Result<()> {
    use anyhow::{Context, bail};
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleProcessList, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
        WriteConsoleInputW,
    };

    let keys = keys(mode, payload).context("nothing to type")?;
    let live = crate::procs::live(pid).context("the agent has exited")?;
    if live.start != start {
        bail!("the agent has exited and another process took its place");
    }
    let ancestors = crate::procs::ancestors_of(pid);
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            bail!("couldn't reach the agent's console");
        }
    }
    let result = (|| -> Result<()> {
        let mut attached = vec![0u32; 64];
        let mut count = unsafe { GetConsoleProcessList(attached.as_mut_ptr(), attached.len() as u32) } as usize;
        if count > attached.len() {
            attached.resize(count + 16, 0);
            count = unsafe { GetConsoleProcessList(attached.as_mut_ptr(), attached.len() as u32) } as usize;
        }
        attached.truncate(count.min(attached.len()));
        if !attached.contains(&pid) {
            bail!("the agent is no longer on its console");
        }
        let native = |other: u32| {
            !program.is_empty()
                && crate::procs::ancestors_of(other).contains(&pid)
                && crate::procs::live(other)
                    .and_then(|l| l.exe)
                    .is_some_and(|exe| exe.rsplit(['\\', '/']).next().unwrap_or(&exe).to_lowercase().starts_with(program))
        };
        if !others_on_console(&attached, std::process::id(), pid, &ancestors, native).is_empty() {
            bail!("something else is running in the agent's terminal, so Lantern won't type there now");
        }
        let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        let input = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if input == INVALID_HANDLE_VALUE {
            bail!("couldn't open the agent's console input");
        }
        let records = |keys: &[Key]| -> Vec<INPUT_RECORD> {
            keys.iter()
                .flat_map(|key| {
                    [true, false].map(|down| INPUT_RECORD {
                        EventType: KEY_EVENT as u16,
                        Event: INPUT_RECORD_0 {
                            KeyEvent: KEY_EVENT_RECORD {
                                bKeyDown: down as i32,
                                wRepeatCount: 1,
                                wVirtualKeyCode: key.vk,
                                wVirtualScanCode: key.scan,
                                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: key.unit },
                                dwControlKeyState: 0,
                            },
                        },
                    })
                })
                .collect()
        };
        let write = |records: &[INPUT_RECORD]| -> bool {
            let mut written = 0u32;
            unsafe {
                WriteConsoleInputW(input, records.as_ptr(), records.len() as u32, &mut written) != 0 && written as usize == records.len()
            }
        };
        let typed = write(&records(&keys));
        // Like a paste followed by Return, so the agent reads the text before it submits.
        let entered = typed && mode == "text" && {
            std::thread::sleep(std::time::Duration::from_millis(60));
            write(&records(&[ENTER]))
        };
        unsafe { CloseHandle(input) };
        match (typed, mode == "text", entered) {
            (false, _, _) => bail!("couldn't type into the agent's console"),
            (true, true, false) => Err(crate::deliver::Uncertain("the text went in but Enter didn't".into()).into()),
            _ => Ok(()),
        }
    })();
    unsafe { FreeConsole() };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_one_line_of_keys_and_single_keys_are_exact() {
        let typed = keys("text", "fix it\nnow é").unwrap();
        let text: String = char::decode_utf16(typed.iter().map(|k| k.unit)).map(|c| c.unwrap()).collect();
        assert_eq!(text, "fix it now é", "newlines would submit early");
        assert!(typed.iter().all(|k| k.vk == 0));
        assert_eq!(keys("key", "escape"), Some(vec![ESCAPE]));
        assert_eq!(keys("key", "1"), Some(vec![Key { vk: 0, scan: 0, unit: b'1' as u16 }]));
        assert_eq!(keys("key", "12"), None);
        assert_eq!(keys("other", "x"), None);
        // A character outside the BMP is two units, as Windows expects.
        assert_eq!(keys("text", "🙂").unwrap().len(), 2);
    }

    #[test]
    fn only_the_agent_and_its_own_shell_may_share_the_console() {
        // helper 9, agent 5, started by shell 4 (in Windows Terminal 2).
        let none = |_| false;
        assert!(others_on_console(&[9, 5, 4], 9, 5, &[4, 2], none).is_empty());
        assert_eq!(others_on_console(&[9, 5, 4, 7], 9, 5, &[4, 2], none), vec![7], "a tool the agent runs");
        // Codex's wrapper (5) runs codex.exe (6); a tool (7) still blocks typing.
        let codex = |pid| pid == 6;
        assert!(others_on_console(&[9, 5, 6, 4], 9, 5, &[4, 2], codex).is_empty());
        assert_eq!(others_on_console(&[9, 5, 6, 7], 9, 5, &[4], codex), vec![7]);
        assert_eq!(native_program(crate::model::Kind::Codex), "codex");
        assert_eq!(native_program(crate::model::Kind::Claude), "");
    }
}
