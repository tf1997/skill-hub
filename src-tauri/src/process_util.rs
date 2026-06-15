use std::{ffi::OsStr, process::Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn hidden_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    hide_window(&mut command);
    command
}

pub fn hide_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
