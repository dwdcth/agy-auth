use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub expiry_timestamp: i64,
    pub token_type: String,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_gcp_tos: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

impl TokenData {
    pub fn new(
        access_token: String,
        refresh_token: String,
        expires_in: i64,
        email: Option<String>,
    ) -> Self {
        let expiry_timestamp = chrono::Utc::now().timestamp() + expires_in;
        Self {
            access_token,
            refresh_token,
            expires_in,
            expiry_timestamp,
            token_type: "Bearer".to_string(),
            email,
            project_id: None,
            is_gcp_tos: None,
            oauth_client_key: None,
            id_token: None,
        }
    }

    pub fn with_oauth_metadata(
        mut self,
        oauth_client_key: Option<String>,
        id_token: Option<String>,
    ) -> Self {
        self.oauth_client_key = oauth_client_key;
        self.id_token = id_token;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub token: TokenData,
    pub quota: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_updated_at: Option<i64>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<i64>,
    pub created_at: i64,
    pub last_used: i64,
}

impl Account {
    pub fn new(id: String, email: String, token: TokenData) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            email,
            name: None,
            tags: Vec::new(),
            notes: None,
            token,
            quota: None,
            tier: None,
            tier_name: None,
            tier_updated_at: None,
            disabled: false,
            disabled_reason: None,
            disabled_at: None,
            created_at: now,
            last_used: now,
        }
    }

    pub fn update_last_used(&mut self) {
        self.last_used = chrono::Utc::now().timestamp();
    }
}

/// Normalize a raw tier id (e.g. "g1-pro-tier", "free-tier") to a canonical
/// lowercase level. Substring matching keeps working when Google renames ids.
pub fn canonicalize_tier(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return "free".to_string();
    }
    if lower.contains("ultra") {
        "ultra".to_string()
    } else if lower.contains("pro") {
        "pro".to_string()
    } else if lower.contains("team") {
        "teams".to_string()
    } else if lower.contains("enterprise") {
        "enterprise".to_string()
    } else if lower.contains("free") || lower.contains("starter") {
        "free".to_string()
    } else {
        lower
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountIndex {
    pub version: String,
    pub accounts: Vec<AccountSummary>,
    pub current_account_id: Option<String>,
}

impl AccountIndex {
    pub fn new() -> Self {
        Self {
            version: "2.0".to_string(),
            accounts: Vec::new(),
            current_account_id: None,
        }
    }
}
