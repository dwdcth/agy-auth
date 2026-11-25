use super::models::Account;

#[derive(serde::Serialize)]
struct AntigravityCredentialToken {
    access_token: String,
    token_type: String,
    refresh_token: String,
    expiry: String,
}

#[derive(serde::Serialize)]
struct AntigravityCredentialPayload {
    token: AntigravityCredentialToken,
    auth_method: String,
}

#[derive(Debug, serde::Deserialize)]
struct StoredAntigravityCredentialToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    expiry: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct StoredAntigravityCredentialPayload {
    token: StoredAntigravityCredentialToken,
    auth_method: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AntigravitySystemCredential {
    pub access_token: Option<String>,
    pub refresh_token: String,
    pub token_type: Option<String>,
    pub expiry: Option<String>,
    pub auth_method: Option<String>,
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_antigravity_credential_secret(secret: &str) -> Result<String, String> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return Err("Antigravity system credential is empty".to_string());
    }

    // Go keyring on macOS stores: go-keyring-base64:<base64(json)>
    if let Some(encoded) = trimmed.strip_prefix("go-keyring-base64:") {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let decoded = STANDARD
            .decode(encoded.trim())
            .map_err(|e| format!("Failed to decode go-keyring payload: {}", e))?;
        return String::from_utf8(decoded)
            .map_err(|e| format!("Invalid UTF-8 in go-keyring payload: {}", e));
    }

    Ok(trimmed.to_string())
}

fn parse_antigravity_system_credential(secret: &str) -> Result<AntigravitySystemCredential, String> {
    let payload_json = normalize_antigravity_credential_secret(secret)?;
    let payload: StoredAntigravityCredentialPayload = serde_json::from_str(&payload_json)
        .map_err(|e| format!("Failed to parse Antigravity system credential: {}", e))?;
    let refresh_token = normalize_non_empty(payload.token.refresh_token.as_deref())
        .ok_or_else(|| "Antigravity system credential missing refresh_token".to_string())?;

    Ok(AntigravitySystemCredential {
        access_token: normalize_non_empty(payload.token.access_token.as_deref()),
        refresh_token,
        token_type: normalize_non_empty(payload.token.token_type.as_deref()),
        expiry: normalize_non_empty(payload.token.expiry.as_deref()),
        auth_method: normalize_non_empty(payload.auth_method.as_deref()),
    })
}

/// Read the currently active Antigravity credential from OS secret storage.
pub fn read_antigravity_system_credential() -> Result<Option<AntigravitySystemCredential>, String> {
    let secret = read_antigravity_system_credential_secret()?;
    match secret {
        Some(secret) => parse_antigravity_system_credential(&secret).map(Some),
        None => Ok(None),
    }
}

fn read_antigravity_system_credential_secret() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
            ])
            .output()
            .map_err(|e| format!("Failed to run security command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Item not found
            if stderr.contains("could not be found") || stderr.contains("SecKeychainSearchCopyNext") {
                return Ok(None);
            }
            return Err(format!("Keychain read failed: {}", stderr.trim()));
        }

        let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if secret.is_empty() {
            Ok(None)
        } else {
            Ok(Some(secret))
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;

        #[repr(C)]
        struct FileTime {
            dw_low_date_time: u32,
            dw_high_date_time: u32,
        }

        #[repr(C)]
        struct CredentialW {
            flags: u32,
            cred_type: u32,
            target_name: *const u16,
            comment: *const u16,
            last_written: FileTime,
            credential_blob_size: u32,
            credential_blob: *const u8,
            persist: u32,
            attribute_count: u32,
            attributes: *const std::ffi::c_void,
            target_alias: *const u16,
            user_name: *const u16,
        }

        #[link(name = "advapi32")]
        extern "system" {
            fn CredReadW(
                target_name: *const u16,
                type_: u32,
                flags: u32,
                credential: *mut *mut CredentialW,
            ) -> i32;
            fn CredFree(buffer: *mut std::ffi::c_void);
        }

        const CRED_TYPE_GENERIC: u32 = 1;
        const ERROR_NOT_FOUND: i32 = 1168;

        let target_wide: Vec<u16> = OsStr::new("gemini:antigravity")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut credential_ptr: *mut CredentialW = ptr::null_mut();

        unsafe {
            if CredReadW(
                target_wide.as_ptr(),
                CRED_TYPE_GENERIC,
                0,
                &mut credential_ptr,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
                    return Ok(None);
                }
                return Err(format!(
                    "Failed to read Windows Credential Manager: {}",
                    error
                ));
            }

            if credential_ptr.is_null() {
                return Ok(None);
            }

            let credential = &*credential_ptr;
            let secret = if credential.credential_blob.is_null() || credential.credential_blob_size == 0
            {
                String::new()
            } else {
                let bytes = std::slice::from_raw_parts(
                    credential.credential_blob,
                    credential.credential_blob_size as usize,
                );
                String::from_utf8_lossy(bytes).trim().to_string()
            };
            CredFree(credential_ptr.cast());

            if secret.is_empty() {
                Ok(None)
            } else {
                Ok(Some(secret))
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let output = match Command::new("secret-tool")
            .args(["lookup", "service", "gemini", "username", "antigravity"])
            .output()
        {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // secret-tool is not installed; the AGY CLI token file is the storage.
                return read_antigravity_cli_token_file();
            }
            Err(e) => return Err(format!("Failed to run secret-tool: {}", e)),
        };

        if output.status.success() {
            let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !secret.is_empty() {
                return Ok(Some(secret));
            }
        }

        // Secret Service returned nothing usable; fall back to the AGY CLI token file.
        read_antigravity_cli_token_file()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Ok(None)
    }
}

fn build_antigravity_credential_payload(account: &Account) -> Result<String, String> {
    let expiry = chrono::DateTime::from_timestamp(account.token.expiry_timestamp, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    serde_json::to_string(&AntigravityCredentialPayload {
        token: AntigravityCredentialToken {
            access_token: account.token.access_token.clone(),
            token_type: "Bearer".to_string(),
            refresh_token: account.token.refresh_token.clone(),
            expiry,
        },
        auth_method: "consumer".to_string(),
    })
    .map_err(|e| format!("Failed to serialize credential payload: {}", e))
}

pub fn write_antigravity_system_credential(account: &Account) -> Result<(), String> {
    let payload_json = build_antigravity_credential_payload(account)?;

    #[cfg(target_os = "macos")]
    {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use std::process::Command;

        let encoded_payload = STANDARD.encode(&payload_json);
        let keychain_value = format!("go-keyring-base64:{}", encoded_payload);

        let _ = Command::new("security")
            .args([
                "delete-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
            ])
            .output();

        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
                &keychain_value,
                "-A",
            ])
            .output()
            .map_err(|e| format!("Failed to run security command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Keychain write failed: {}", stderr.trim()));
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;

        #[repr(C)]
        struct FileTime {
            dw_low_date_time: u32,
            dw_high_date_time: u32,
        }

        #[repr(C)]
        struct CredentialW {
            flags: u32,
            cred_type: u32,
            target_name: *const u16,
            comment: *const u16,
            last_written: FileTime,
            credential_blob_size: u32,
            credential_blob: *const u8,
            persist: u32,
            attribute_count: u32,
            attributes: *const std::ffi::c_void,
            target_alias: *const u16,
            user_name: *const u16,
        }

        #[link(name = "advapi32")]
        extern "system" {
            fn CredDeleteW(target_name: *const u16, type_: u32, flags: u32) -> i32;
            fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        }

        const CRED_TYPE_GENERIC: u32 = 1;
        const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

        let target_wide: Vec<u16> = OsStr::new("gemini:antigravity")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let user_wide: Vec<u16> = OsStr::new("antigravity")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let secret = payload_json.as_bytes();

        let credential = CredentialW {
            flags: 0,
            cred_type: CRED_TYPE_GENERIC,
            target_name: target_wide.as_ptr(),
            comment: ptr::null(),
            last_written: FileTime {
                dw_low_date_time: 0,
                dw_high_date_time: 0,
            },
            credential_blob_size: secret.len() as u32,
            credential_blob: secret.as_ptr(),
            persist: CRED_PERSIST_LOCAL_MACHINE,
            attribute_count: 0,
            attributes: ptr::null(),
            target_alias: ptr::null(),
            user_name: user_wide.as_ptr(),
        };

        unsafe {
            let _ = CredDeleteW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0);
            if CredWriteW(&credential, 0) == 0 {
                return Err(format!(
                    "Failed to write to Windows Credential Manager: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use colored::Colorize;

        // The AGY CLI reads this file directly when no Secret Service is available
        // (headless Linux), so always keep it in sync with the active account.
        write_antigravity_cli_token_file(account)?;

        if let Err(e) = write_antigravity_credential_via_secret_tool(&payload_json) {
            eprintln!(
                "{} Secret Service unavailable ({}); only the AGY CLI token file at {} was updated. Install libsecret-tools and gnome-keyring for native keyring storage.",
                "Warning:".yellow().bold(),
                e,
                antigravity_cli_token_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "<unknown>".to_string())
            );
        }
    }

    Ok(())
}

/// Location of the AGY CLI's own file-based credential fallback on Linux.
#[cfg(target_os = "linux")]
fn antigravity_cli_token_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Unable to get home directory")?;
    Ok(home
        .join(".gemini")
        .join("antigravity-cli")
        .join("antigravity-oauth-token"))
}

#[cfg(target_os = "linux")]
fn read_antigravity_cli_token_file() -> Result<Option<String>, String> {
    let path = antigravity_cli_token_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read AGY CLI token file: {}", e)),
    }
}

#[cfg(target_os = "linux")]
fn extract_id_token_email(id_token: Option<&str>) -> Option<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let payload_b64 = id_token?.trim().split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload_b64))
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("email")?
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(ToOwned::to_owned)
}

/// Payload format written by the AGY CLI itself to `antigravity-oauth-token`.
#[cfg(target_os = "linux")]
#[derive(serde::Serialize)]
struct AntigravityCliTokenFile {
    token: AntigravityCliTokenFileToken,
    auth_method: String,
    id_token_email: String,
}

#[cfg(target_os = "linux")]
#[derive(serde::Serialize)]
struct AntigravityCliTokenFileToken {
    access_token: String,
    token_type: String,
    refresh_token: String,
    expiry: String,
    expires_in: i64,
}

#[cfg(target_os = "linux")]
fn write_antigravity_cli_token_file(account: &Account) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let expiry = chrono::DateTime::from_timestamp(account.token.expiry_timestamp, 0)
        .unwrap_or_else(chrono::Utc::now);
    let expiry_local = chrono::DateTime::<chrono::Local>::from(expiry)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, false);
    let expires_in = (account.token.expiry_timestamp - chrono::Utc::now().timestamp()).max(0);

    let id_token_email = extract_id_token_email(account.token.id_token.as_deref())
        .unwrap_or_else(|| account.email.clone());

    let payload = serde_json::to_string(&AntigravityCliTokenFile {
        token: AntigravityCliTokenFileToken {
            access_token: account.token.access_token.clone(),
            token_type: account.token.token_type.clone(),
            refresh_token: account.token.refresh_token.clone(),
            expiry: expiry_local,
            expires_in,
        },
        auth_method: "consumer".to_string(),
        id_token_email,
    })
    .map_err(|e| format!("Failed to serialize AGY CLI token file: {}", e))?;

    let path = antigravity_cli_token_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!("Failed to create AGY CLI token directory: {}", e)
        })?;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| format!("Failed to create AGY CLI token file: {}", e))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Failed to restrict AGY CLI token permissions: {}", e))?;
    file.write_all(payload.as_bytes())
        .map_err(|e| format!("Failed to write AGY CLI token file: {}", e))?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn write_antigravity_credential_via_secret_tool(payload_json: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label=gemini",
            "service",
            "gemini",
            "username",
            "antigravity",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn secret-tool: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload_json.as_bytes())
            .map_err(|e| format!("failed to write to secret-tool: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for secret-tool: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("secret-tool write failed: {}", stderr.trim()));
    }

    Ok(())
}
