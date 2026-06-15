use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::process_util::hidden_command;

pub const ADMIN_KEY: &str = "skillhub-admin";
pub const MINIO_PUBLISHER_ACCESS_KEY: &str = "minioadmin";
pub const MINIO_PUBLISHER_SECRET_KEY: &str = "minioadmin";
pub const MAC_ALLOWLIST_OBJECT_PATH: &str = "admin/security/mac-allowlist.v1.json";

#[derive(Debug, Deserialize)]
pub struct MacAllowlist {
    #[serde(default)]
    pub macs: Vec<String>,
    #[serde(default)]
    pub entries: Vec<MacAllowlistEntry>,
}

#[derive(Debug, Deserialize)]
pub struct MacAllowlistEntry {
    #[serde(alias = "macAddress", alias = "mac_address")]
    pub mac: Option<String>,
    pub status: Option<String>,
    pub role: Option<String>,
    #[serde(default, alias = "projectSlugs", alias = "authorizedProjects")]
    pub projects: Vec<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAuthorization {
    pub role: String,
    pub projects: Vec<String>,
    pub mac_address: String,
    pub name: Option<String>,
}

impl AdminAuthorization {
    pub fn is_system(&self) -> bool {
        self.role == "system"
    }

    pub fn can_manage_project(&self, _slug: &str) -> bool {
        self.role == "system" || self.role == "project"
    }
}

pub fn is_admin_key_valid(value: &str) -> bool {
    value.trim() == ADMIN_KEY
}

pub fn local_mac_addresses() -> Vec<String> {
    let output = platform_mac_output();
    extract_mac_addresses(&output)
}

pub fn allowlist_path() -> &'static str {
    MAC_ALLOWLIST_OBJECT_PATH
}

pub fn parse_mac_allowlist(content: &str) -> Result<MacAllowlist, serde_json::Error> {
    serde_json::from_str(content)
}

pub fn admin_authorization(
    local_macs: &[String],
    allowlist: &MacAllowlist,
) -> Option<AdminAuthorization> {
    let local = local_macs
        .iter()
        .filter_map(|mac| normalize_mac(mac))
        .collect::<HashSet<_>>();
    if local.is_empty() {
        return None;
    }

    for mac in &allowlist.macs {
        let Some(normalized) = normalize_mac(mac) else {
            continue;
        };
        if local.contains(&normalized) {
            return Some(AdminAuthorization {
                role: "system".to_string(),
                projects: vec!["*".to_string()],
                mac_address: normalized,
                name: None,
            });
        }
    }

    let mut project_match = None;
    for entry in &allowlist.entries {
        let status = entry.status.as_deref().unwrap_or("active");
        if !status.eq_ignore_ascii_case("active") {
            continue;
        }
        let Some(normalized) = entry.mac.as_deref().and_then(normalize_mac) else {
            continue;
        };
        if !local.contains(&normalized) {
            continue;
        }

        let role = normalize_role(entry.role.as_deref());
        let projects = normalize_projects(&entry.projects);
        let authorization = AdminAuthorization {
            role,
            projects,
            mac_address: normalized,
            name: entry.name.clone(),
        };
        if authorization.is_system() {
            return Some(authorization);
        }
        if project_match.is_none() {
            project_match = Some(authorization);
        }
    }

    project_match
}

fn normalize_role(value: Option<&str>) -> String {
    match value
        .unwrap_or("project")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "project" | "project_admin" | "project-admin" => "project".to_string(),
        _ => "system".to_string(),
    }
}

fn normalize_projects(projects: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = projects
        .iter()
        .map(|project| project.trim().to_string())
        .filter(|project| !project.is_empty())
        .filter(|project| seen.insert(project.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
}

pub fn publisher_access_key() -> &'static str {
    MINIO_PUBLISHER_ACCESS_KEY
}

pub fn publisher_secret_key() -> &'static str {
    MINIO_PUBLISHER_SECRET_KEY
}

fn platform_mac_output() -> String {
    #[cfg(target_os = "windows")]
    {
        return [
            command_stdout("getmac", &["/FO", "CSV", "/NH"]),
            command_stdout(
                "powershell",
                &[
                    "-NoProfile",
                    "-Command",
                    "Get-CimInstance Win32_NetworkAdapterConfiguration | Where-Object { $_.MACAddress } | ForEach-Object { $_.MACAddress }",
                ],
            ),
            command_stdout("ipconfig", &["/all"]),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    }

    #[cfg(not(target_os = "windows"))]
    {
        [
            command_stdout("ifconfig", &["-a"]),
            command_stdout("ip", &["link"]),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = hidden_command(program).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_mac_addresses(output: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut macs = Vec::new();

    for token in
        output.split(|ch: char| !(ch.is_ascii_hexdigit() || ch == '-' || ch == ':' || ch == '.'))
    {
        let Some(mac) = normalize_mac(token) else {
            continue;
        };

        if seen.insert(mac.clone()) {
            macs.push(mac);
        }
    }

    macs
}

fn normalize_mac(value: &str) -> Option<String> {
    let hex = value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<String>();

    if hex.len() != 12 || hex == "000000000000" {
        return None;
    }

    let mut normalized = String::with_capacity(17);
    for (index, ch) in hex.chars().enumerate() {
        if index > 0 && index % 2 == 0 {
            normalized.push(':');
        }
        normalized.push(ch);
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_entry_mac_allowlist() {
        let allowlist = parse_mac_allowlist(
            r#"{
              "macs": ["aa-bb-cc-dd-ee-ff"],
              "entries": [
                { "macAddress": "11:22:33:44:55:66", "status": "disabled" },
                { "mac": "33:44:55:66:77:88" },
                { "mac_address": "22.33.44.55.66.77", "role": "project", "projects": ["live-project"] }
              ]
            }"#,
        )
        .expect("allowlist should parse");

        assert!(admin_authorization(&[String::from("AA:BB:CC:DD:EE:FF")], &allowlist).is_some());
        assert!(admin_authorization(&[String::from("22-33-44-55-66-77")], &allowlist).is_some());
        let authorization = admin_authorization(&[String::from("22-33-44-55-66-77")], &allowlist)
            .expect("project admin should match");
        assert_eq!(authorization.role, "project");
        assert!(authorization.can_manage_project("live-project"));
        assert!(authorization.can_manage_project("other-project"));
        let default_entry_authorization =
            admin_authorization(&[String::from("33-44-55-66-77-88")], &allowlist)
                .expect("entry without role should match");
        assert_eq!(default_entry_authorization.role, "project");
        assert!(default_entry_authorization.can_manage_project("any-project"));
        assert!(admin_authorization(&[String::from("11-22-33-44-55-66")], &allowlist).is_none());
    }
}
