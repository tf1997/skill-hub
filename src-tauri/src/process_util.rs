use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    process::Command,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn hidden_command<S: AsRef<OsStr>>(program: S) -> Command {
    external_command(program)
}

pub fn external_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    hide_window(&mut command);
    apply_external_command_env(&mut command);
    command
}

pub fn hide_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn apply_external_command_env(command: &mut Command) {
    #[cfg(windows)]
    {
        if let Some(path) = windows_merged_path_env() {
            command.env("PATH", path);
        }
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(windows)]
fn windows_merged_path_env() -> Option<OsString> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let extras = [
        std::env::var_os("USERPROFILE").map(|home| {
            std::path::PathBuf::from(home)
                .join("AppData")
                .join("Roaming")
                .join("npm")
                .into_os_string()
        }),
        std::env::var_os("APPDATA").map(|appdata| {
            std::path::PathBuf::from(appdata)
                .join("npm")
                .into_os_string()
        }),
        Some(OsString::from(r"C:\Program Files\nodejs")),
    ]
    .into_iter()
    .flatten();
    Some(merge_path_env_values(current, extras))
}

fn merge_path_env_values<I>(base: OsString, extras: I) -> OsString
where
    I: IntoIterator<Item = OsString>,
{
    let mut seen = HashSet::new();
    let mut parts = std::env::split_paths(&base).collect::<Vec<_>>();
    for part in &parts {
        seen.insert(path_key(part.as_os_str()));
    }
    for extra in extras {
        if extra.is_empty() {
            continue;
        }
        let key = path_key(extra.as_os_str());
        if seen.insert(key) {
            parts.push(extra.into());
        }
    }
    std::env::join_paths(parts).unwrap_or(base)
}

fn path_key(path: &OsStr) -> String {
    #[cfg(windows)]
    {
        path.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase()
    }

    #[cfg(not(windows))]
    {
        path.to_string_lossy().trim_end_matches('/').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn merged_path_env_appends_missing_entries_case_insensitively() {
        let merged = merge_path_env_values(
            OsString::from(r"C:\Tools;C:\Users\ctf19\AppData\Roaming\npm"),
            [
                OsString::from(r"C:\tools"),
                OsString::from(r"C:\Program Files\nodejs"),
            ],
        );
        let merged = merged.to_string_lossy();
        assert!(merged.contains(r"C:\Users\ctf19\AppData\Roaming\npm"));
        assert!(merged.contains(r"C:\Program Files\nodejs"));
        assert_eq!(merged.matches(r"C:\Tools").count(), 1);
    }
}
