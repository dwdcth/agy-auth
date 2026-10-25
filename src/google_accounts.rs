use serde_json::{Map, Value};
use std::io::Write;
use std::path::Path;

/// Synchronize Google's account-selection cache, not OAuth credentials.
pub fn sync_active_account(email: &str) -> anyhow::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Unable to get home directory"))?;
    sync_active_account_at(&home.join(".gemini/google_accounts.json"), email)
}

fn updated_accounts(value: Value, email: &str) -> anyhow::Result<Value> {
    let email = email.trim();
    anyhow::ensure!(!email.is_empty(), "Active account email is empty");
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Google accounts cache must be a JSON object"))?;
    let active = match object.get("active") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.trim().to_owned()),
        _ => anyhow::bail!("Google accounts cache active must be a string or null"),
    };
    let previous = match object.get("old") {
        None => Vec::new(),
        Some(Value::Array(values)) => values.clone(),
        _ => anyhow::bail!("Google accounts cache old must be an array"),
    };
    let mut old: Vec<String> = Vec::new();
    for entry in previous {
        let entry = entry
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Google accounts cache old entries must be strings"))?
            .trim();
        if !entry.is_empty()
            && !entry.eq_ignore_ascii_case(email)
            && !old
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(entry))
        {
            old.push(entry.to_owned());
        }
    }
    if let Some(active) = active {
        if !active.is_empty()
            && !active.eq_ignore_ascii_case(email)
            && !old
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&active))
        {
            old.push(active);
        }
    }
    object.insert("active".into(), Value::String(email.to_owned()));
    object.insert("old".into(), serde_json::to_value(old)?);
    Ok(Value::Object(object))
}

fn sync_active_account_at(path: &Path, email: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    let value = match std::fs::read(path) {
        Ok(content) => serde_json::from_slice(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()))
        }
    };
    // Validate before creating directories or replacing any existing file.
    let updated = updated_accounts(value, email)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing cache parent directory"))?;
    std::fs::create_dir_all(parent)?;
    // A same-directory temporary file allows atomic replacement on supported platforms.
    // NamedTempFile creates private (0600) files on Unix and cleans up on failure.
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &updated)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn switching_preserves_metadata_and_normalizes_history() {
        let result = updated_accounts(
            json!({
                "active": "previous@example.com",
                "old": ["TARGET@example.com", "older@example.com", "OLDER@example.com", ""],
                "extra": {"keep": true}
            }),
            "target@example.com",
        )
        .unwrap();
        assert_eq!(
            result,
            json!({
                "active": "target@example.com",
                "old": ["older@example.com", "previous@example.com"],
                "extra": {"keep": true}
            })
        );
    }

    #[test]
    fn switching_to_same_account_is_idempotent() {
        let initial = json!({"active": "current@example.com", "old": ["older@example.com"]});
        assert_eq!(
            updated_accounts(initial.clone(), "current@example.com").unwrap(),
            initial
        );
    }

    #[test]
    fn creates_missing_cache_and_replaces_existing_cache() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".gemini/google_accounts.json");
        sync_active_account_at(&path, "first@example.com").unwrap();
        sync_active_account_at(&path, "second@example.com").unwrap();
        let actual: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            actual,
            json!({"active": "second@example.com", "old": ["first@example.com"]})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn malformed_cache_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("google_accounts.json");
        for original in ["broken", "[]", "{\"old\":123}", "{\"old\":[null]}"] {
            std::fs::write(&path, original).unwrap();
            assert!(sync_active_account_at(&path, "target@example.com").is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    fn supports_null_active_and_rejects_empty_email() {
        assert_eq!(
            updated_accounts(json!({"active": null}), "target@example.com").unwrap(),
            json!({"active": "target@example.com", "old": []})
        );
        assert!(updated_accounts(json!({}), "  ").is_err());
    }
}
