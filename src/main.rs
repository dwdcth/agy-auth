mod models;
mod oauth;
mod credential;
mod storage;
mod ide_db;
mod protobuf;
mod import;
mod google_accounts;

use clap::{Parser, Subcommand};
use colored::*;
use models::{Account, TokenData};
use std::path::PathBuf;
use std::process::Command;
use tabled::{Table, Tabled};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn color_tier(tier: &str) -> colored::ColoredString {
    match tier {
        "ultra" => tier.bright_magenta().bold(),
        "pro" => tier.bright_cyan(),
        "free" => tier.white(),
        other => other.normal(),
    }
}

#[derive(Parser)]
#[command(name = "agy-auth")]
#[command(author, version, about = "AGY CLI Multi-Account Management Tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all Antigravity/AGY accounts
    #[command(alias = "ls")]
    List,

    /// Add a new account (using interactive browser login or refresh token)
    #[command(alias = "login")]
    Add {
        /// Optional: Google OAuth refresh token. If not provided, starts interactive browser login.
        refresh_token: Option<String>,
    },

    /// Import accounts from local Antigravity credentials or a JSON file
    Import {
        /// Path to a JSON file to import (cockpit/agy-auth export formats supported)
        path: Option<PathBuf>,

        /// Import the currently logged-in Antigravity / AGY account from this machine
        #[arg(long, short = 'l', conflicts_with = "path")]
        local: bool,
    },

    /// Switch active account
    #[command(alias = "use")]
    Switch {
        /// The account ID or Email to switch to
        account: Option<String>,
    },

    /// Show the currently active account details
    #[command(alias = "whoami")]
    Current,

    /// Delete an account from the local registry
    #[command(alias = "remove")]
    Delete {
        /// The account ID or Email to delete
        account: String,
    },

    /// Add a tag to one or more accounts
    Tag {
        /// Apply the tag to ALL accounts
        #[arg(long, conflicts_with = "ids")]
        all: bool,

        /// Comma-separated account IDs (row numbers from `agy-auth list`), e.g. 1,3,5
        #[arg(long = "id", value_delimiter = ',', required_unless_present = "all")]
        ids: Vec<usize>,

        /// The tag to add; an empty value clears all tags instead
        tag: String,
    },

    /// Detect or set the subscription tier (free / pro / ultra) of accounts
    #[command(alias = "plan")]
    Tier {
        /// The account ID or Email. Omit to refresh ALL accounts.
        account: Option<String>,

        /// Manually set the tier instead of auto-detecting (e.g. free, pro, ultra)
        #[arg(long, short = 's', requires = "account")]
        set: Option<String>,

        /// Clear the stored tier of the account
        #[arg(long, requires = "account", conflicts_with = "set")]
        clear: bool,
    },
}

#[derive(Tabled)]
struct AccountDisplay {
    #[tabled(rename = "ID")]
    num: usize,
    #[tabled(rename = "Active")]
    active: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Email")]
    email: String,
    #[tabled(rename = "Tier")]
    tier: String,
    #[tabled(rename = "Tags")]
    tags: String,
    #[tabled(rename = "UUID")]
    uuid: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            if let Err(e) = list_accounts().await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Add { refresh_token } => {
            if let Err(e) = add_account(refresh_token).await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Import { path, local } => {
            if let Err(e) = import_accounts(path, local).await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Switch { account } => {
            if let Err(e) = switch_account(account.as_deref()).await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Current => {
            if let Err(e) = show_current_account().await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Delete { account } => {
            if let Err(e) = delete_account(&account).await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Tag { all, ids, tag } => {
            if let Err(e) = tag_command(all, &ids, &tag) {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
        Commands::Tier {
            account,
            set,
            clear,
        } => {
            if let Err(e) = tier_command(account.as_deref(), set.as_deref(), clear).await {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
    }

    Ok(())
}

async fn import_accounts(path: Option<PathBuf>, local: bool) -> anyhow::Result<()> {
    enum ImportMode {
        Local,
        Json(PathBuf),
    }

    let mode = if local {
        ImportMode::Local
    } else if let Some(path) = path {
        ImportMode::Json(path)
    } else {
        use dialoguer::{theme::ColorfulTheme, Select};

        let items = [
            "Local Antigravity / AGY account (system keychain, CLI token, or IDE DB)",
            "JSON file",
        ];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select import source")
            .default(0)
            .items(&items)
            .interact_opt()
            .map_err(|e| anyhow::anyhow!("Failed to read selection: {}", e))?;

        match selection {
            Some(0) => ImportMode::Local,
            Some(1) => {
                let path: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Path to JSON file")
                    .interact_text()
                    .map_err(|e| anyhow::anyhow!("Failed to read path: {}", e))?;
                ImportMode::Json(PathBuf::from(path.trim()))
            }
            _ => {
                println!("Import cancelled.");
                return Ok(());
            }
        }
    };

    match mode {
        ImportMode::Local => {
            println!("Importing local Antigravity account...");
            let account = import::import_from_local()
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            println!(
                "{} Imported account: {}",
                "Success:".green().bold(),
                account.email.bright_green()
            );
            println!(
                "Use `agy-auth switch {}` to make it active.",
                account.email.cyan()
            );
        }
        ImportMode::Json(path) => {
            if !path.exists() {
                return Err(anyhow::anyhow!("JSON file not found: {}", path.display()));
            }
            println!("Importing from JSON: {}", path.display());
            let summary = import::import_from_json_file(&path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            if summary.imported.is_empty() && summary.failed.is_empty() {
                println!("No accounts imported.");
            } else {
                println!(
                    "{} Imported {} account(s).",
                    "Success:".green().bold(),
                    summary.imported.len()
                );
                for acc in &summary.imported {
                    println!("  + {}", acc.email.bright_green());
                }
            }

            if !summary.failed.is_empty() {
                println!(
                    "{} {} account(s) failed:",
                    "Warning:".yellow().bold(),
                    summary.failed.len()
                );
                for (label, err) in &summary.failed {
                    println!("  - {}: {}", label, err);
                }
            }

            if let Some(first) = summary.imported.first() {
                println!(
                    "Use `agy-auth switch {}` to make an account active.",
                    first.email.cyan()
                );
            }
        }
    }

    Ok(())
}

async fn list_accounts() -> anyhow::Result<()> {
    let accounts = storage::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let current_id = storage::get_current_account_id();

    let display_list: Vec<AccountDisplay> = accounts
        .into_iter()
        .enumerate()
        .map(|(idx, acc)| {
            let active = if Some(acc.id.clone()) == current_id {
                "  *  ".green().bold().to_string()
            } else {
                "".to_string()
            };
            AccountDisplay {
                num: idx + 1,
                active,
                name: acc.name.unwrap_or_default(),
                email: acc.email,
                tier: acc.tier.as_deref().map(color_tier).map(|c| c.to_string()).unwrap_or_else(|| "-".to_string()),
                tags: acc.tags.join(", "),
                uuid: acc.id,
            }
        })
        .collect();

    if display_list.is_empty() {
        println!("No AGY accounts registered. Use `agy-auth add` to log in.");
    } else {
        println!("{}", Table::new(display_list).to_string());
    }

    Ok(())
}

async fn add_account(refresh_token: Option<String>) -> anyhow::Result<()> {
    let token = match refresh_token {
        Some(rt) => rt,
        None => {
            // Interactive login via browser callback
            run_interactive_login().await?
        }
    };

    println!("Authenticating with Google OAuth...");
    let token_res = oauth::refresh_access_token(&token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to exchange token: {}", e))?;
    let user_info = oauth::get_user_info(&token_res.access_token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to retrieve user info: {}", e))?;

    let token_data = TokenData::new(
        token_res.access_token,
        token.clone(),
        token_res.expires_in,
        Some(user_info.email.clone()),
    )
    .with_oauth_metadata(token_res.oauth_client_key, token_res.id_token);

    let email = user_info.email.clone();
    let display_name = user_info.get_display_name();

    let account = storage::upsert_account(email.clone(), display_name, token_data)
        .map_err(|e| anyhow::anyhow!("Failed to register account: {}", e))?;

    println!("Successfully registered account: {}", email.bright_green());

    // Best-effort tier detection for the freshly added account.
    if let Ok(mut saved) = storage::load_account(&account.id) {
        match refresh_account_tier(&mut saved).await {
            Ok(()) => println!(
                "Detected tier: {}",
                color_tier(saved.tier.as_deref().unwrap_or("-"))
            ),
            Err(e) => println!("{} Tier detection skipped: {}", "Note:".yellow(), e),
        }
    }

    println!("Use `agy-auth switch {}` to switch to this account.", email.cyan());

    Ok(())
}

fn find_account<'a>(accounts: &'a [Account], reference: &str) -> Option<&'a Account> {
    accounts
        .iter()
        .find(|acc| acc.id == reference || acc.email.eq_ignore_ascii_case(reference))
}

/// Refresh the account token if needed, query its subscription tier, and
/// persist both to the account file.
pub(crate) async fn refresh_account_tier(account: &mut Account) -> Result<(), String> {
    let fresh_token = oauth::ensure_fresh_token(&account.token).await?;
    if fresh_token.access_token != account.token.access_token {
        account.token = fresh_token;
    }

    let tier_info = oauth::fetch_tier(&account.token.access_token).await?;
    account.tier = Some(tier_info.tier);
    account.tier_name = tier_info.name;
    account.tier_updated_at = Some(chrono::Utc::now().timestamp());

    storage::save_account(account)
}

fn tier_display(account: &Account) -> String {
    match (&account.tier, &account.tier_name) {
        (Some(t), Some(n)) => format!("{} ({})", color_tier(t), n),
        (Some(t), None) => color_tier(t).to_string(),
        _ => "-".to_string(),
    }
}

fn tag_command(all: bool, ids: &[usize], tag: &str) -> anyhow::Result<()> {
    let accounts = storage::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    if accounts.is_empty() {
        return Err(anyhow::anyhow!(
            "No registered accounts found. Please add an account using `agy-auth add` first."
        ));
    }

    // Row numbers shown by `agy-auth list`: enumerate(list_accounts order) + 1.
    let rows: std::collections::BTreeSet<usize> = if all {
        (1..=accounts.len()).collect()
    } else {
        let mut rows = std::collections::BTreeSet::new();
        for num in ids {
            if *num == 0 || *num > accounts.len() {
                return Err(anyhow::anyhow!(
                    "Invalid account ID {}: valid range is 1-{} (see `agy-auth list`)",
                    num,
                    accounts.len()
                ));
            }
            rows.insert(*num);
        }
        rows
    };

    // An empty tag clears all tags of the target accounts.
    let clearing = tag.trim().is_empty();

    let mut changed = 0usize;
    let mut unchanged = 0usize;
    for num in rows {
        let mut account = accounts[num - 1].clone();
        if clearing {
            if account.tags.is_empty() {
                println!("  = {} (no tags)", account.email);
                unchanged += 1;
                continue;
            }
            account.tags.clear();
            storage::save_account(&account).map_err(|e| anyhow::anyhow!(e))?;
            println!("  - {} (cleared)", account.email.bright_green());
            changed += 1;
        } else {
            let tag = tag.trim();
            if account.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                println!("  = {} (already tagged)", account.email);
                unchanged += 1;
                continue;
            }
            account.tags.push(tag.to_string());
            storage::save_account(&account).map_err(|e| anyhow::anyhow!(e))?;
            println!("  + {} [{}]", account.email.bright_green(), tag);
            changed += 1;
        }
    }

    if clearing {
        println!(
            "{} Cleared tags on {} account(s) ({} had none).",
            "Success:".green().bold(),
            changed,
            unchanged
        );
    } else {
        println!(
            "{} Tagged {} account(s) with '{}' ({} already had it).",
            "Success:".green().bold(),
            changed,
            tag.trim(),
            unchanged
        );
    }

    Ok(())
}

async fn tier_command(
    account_ref: Option<&str>,
    set: Option<&str>,
    clear: bool,
) -> anyhow::Result<()> {
    let accounts = storage::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    if accounts.is_empty() {
        return Err(anyhow::anyhow!(
            "No registered accounts found. Please add an account using `agy-auth add` first."
        ));
    }

    if let Some(value) = set {
        let account_ref = account_ref.expect("--set requires an account");
        let target = find_account(&accounts, account_ref)
            .ok_or_else(|| anyhow::anyhow!("Account not found matching: '{}'", account_ref))?;

        let mut account = target.clone();
        account.tier = Some(models::canonicalize_tier(value));
        account.tier_name = None;
        account.tier_updated_at = Some(chrono::Utc::now().timestamp());
        storage::save_account(&account).map_err(|e| anyhow::anyhow!(e))?;
        println!(
            "{} Set tier of {} to {}",
            "Success:".green().bold(),
            account.email.bright_green(),
            color_tier(account.tier.as_deref().unwrap_or("-"))
        );
        return Ok(());
    }

    if clear {
        let account_ref = account_ref.expect("--clear requires an account");
        let target = find_account(&accounts, account_ref)
            .ok_or_else(|| anyhow::anyhow!("Account not found matching: '{}'", account_ref))?;

        let mut account = target.clone();
        account.tier = None;
        account.tier_name = None;
        account.tier_updated_at = None;
        storage::save_account(&account).map_err(|e| anyhow::anyhow!(e))?;
        println!(
            "{} Cleared tier of {}",
            "Success:".green().bold(),
            account.email.bright_green()
        );
        return Ok(());
    }

    // Auto-detect: single account or all of them.
    let targets: Vec<Account> = match account_ref {
        Some(r) => {
            let target = find_account(&accounts, r)
                .ok_or_else(|| anyhow::anyhow!("Account not found matching: '{}'", r))?;
            vec![target.clone()]
        }
        None => accounts,
    };

    let total = targets.len();
    println!(
        "Detecting subscription tier for {} account(s)...",
        total
    );
    let mut updated = 0usize;
    let mut failed = 0usize;
    for mut account in targets {
        match refresh_account_tier(&mut account).await {
            Ok(()) => {
                let name_suffix = account
                    .tier_name
                    .as_deref()
                    .map(|n| format!(" ({})", n))
                    .unwrap_or_default();
                println!(
                    "  {} -> {}{}",
                    account.email.bright_green(),
                    color_tier(account.tier.as_deref().unwrap_or("-")),
                    name_suffix.dimmed()
                );
                updated += 1;
            }
            Err(e) => {
                println!("  {} -> {}", account.email.yellow(), format!("failed: {}", e).red());
                failed += 1;
            }
        }
    }

    if total > 1 {
        println!(
            "{} {} updated, {} failed.",
            "Done:".green().bold(),
            updated,
            failed
        );
    }

    Ok(())
}

async fn run_interactive_login() -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind to local port: {}", e))?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{}/oauth-callback", port);
    let expected_state = uuid::Uuid::new_v4().to_string();

    let auth_url = oauth::get_auth_url(&redirect_uri, Some(&expected_state));

    println!("{}\n", "Please open the following link in your browser to authorize AGY CLI:".bright_blue().bold());
    println!("  {}", auth_url.underline().cyan());
    println!("\nWaiting for browser callback on port {}...", port);

    open_browser(&auth_url);

    let timeout_dur = tokio::time::Duration::from_secs(300);

    tokio::select! {
        res = accept_and_extract_code(listener, &expected_state, port) => {
            let auth_code = res?;
            println!("Authorization code received! Exchanging code...");
            let token_res = oauth::exchange_code(&auth_code, &redirect_uri)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to exchange authorization code: {}", e))?;
            let refresh_token = token_res.refresh_token.ok_or_else(|| {
                anyhow::anyhow!("Google did not return a refresh token. If you've logged in before, please revoke access first or pass the refresh token manually.")
            })?;
            Ok(refresh_token)
        }
        _ = tokio::time::sleep(timeout_dur) => {
            Err(anyhow::anyhow!("Interactive login timed out after 5 minutes"))
        }
    }
}

async fn accept_and_extract_code(
    listener: tokio::net::TcpListener,
    expected_state: &str,
    port: u16,
) -> anyhow::Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buffer = [0u8; 4096];
        let bytes_read = stream.read(&mut buffer).await?;
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);

        let first_line = match request.lines().next() {
            Some(line) => line,
            None => continue,
        };
        let mut parts = first_line.split_whitespace();
        let method = parts.next();
        let target = parts.next();

        if let (Some("GET"), Some(path)) = (method, target) {
            let full_url = format!("http://localhost:{}{}", port, path);
            if let Ok(url) = url::Url::parse(&full_url) {
                if url.path() == "/oauth-callback" {
                    let mut code = None;
                    let mut state = None;
                    for (key, val) in url.query_pairs() {
                        if key == "code" {
                            code = Some(val.into_owned());
                        } else if key == "state" {
                            state = Some(val.into_owned());
                        }
                    }

                    if let (Some(c), Some(s)) = (code, state) {
                        if s != expected_state {
                            let response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nState mismatch error.";
                            let _ = stream.write_all(response.as_bytes()).await;
                            return Err(anyhow::anyhow!("OAuth state mismatch"));
                        }

                        let html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
                        <html>\
                        <body style='font-family: sans-serif; text-align: center; padding: 50px; background: #0d1117; color: #fff;'>\
                            <h1 style='color: #4ade80;'>\u{2705} 授权成功!</h1>\
                            <p>您已成功登录，可以关闭此窗口返回终端。</p>\
                            <script>setTimeout(function() { window.close(); }, 2000);</script>\
                        </body>\
                        </html>";
                        let _ = stream.write_all(html.as_bytes()).await;
                        let _ = stream.flush().await;
                        return Ok(c);
                    }
                }
            }
        }

        let response = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nNot Found";
        let _ = stream.write_all(response.as_bytes()).await;
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/c", "start", url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();
}

async fn switch_account(account_ref: Option<&str>) -> anyhow::Result<()> {
    let accounts = storage::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    if accounts.is_empty() {
        return Err(anyhow::anyhow!(
            "No registered accounts found. Please add an account using `agy-auth add` first."
        ));
    }

    let target = match account_ref {
        Some(r) => accounts
            .into_iter()
            .find(|acc| acc.id == r || acc.email.eq_ignore_ascii_case(r))
            .ok_or_else(|| anyhow::anyhow!("Account not found matching: '{}'", r))?,
        None => {
            use dialoguer::{theme::ColorfulTheme, Select};

            let current_id = storage::get_current_account_id();
            let mut items = Vec::new();
            let mut default_idx = 0;

            for (idx, acc) in accounts.iter().enumerate() {
                let mut label = acc.email.clone();
                if let Some(ref name) = acc.name {
                    if !name.is_empty() {
                        label = format!("{} ({})", label, name);
                    }
                }
                if Some(acc.id.clone()) == current_id {
                    label = format!("{} [Active]", label);
                    default_idx = idx;
                }
                if let Some(tier) = &acc.tier {
                    label = format!("{} [{}]", label, color_tier(tier));
                }
                items.push(label);
            }

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select the account to switch to")
                .default(default_idx)
                .items(&items)
                .interact_opt()
                .map_err(|e| anyhow::anyhow!("Failed to read selection: {}", e))?;

            match selection {
                Some(idx) => accounts.into_iter().nth(idx).unwrap(),
                None => {
                    println!("Switch cancelled.");
                    return Ok(());
                }
            }
        }
    };

    switch_to_account_object(&target).await
}

async fn switch_to_account_object(target: &Account) -> anyhow::Result<()> {
    let mut account = target.clone();
    
    // Ensure token is fresh
    let fresh_token = oauth::ensure_fresh_token(&account.token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to refresh access token: {}", e))?;
    if fresh_token.access_token != account.token.access_token {
        account.token = fresh_token;
    }

    // Persist refreshed credentials before changing the client's active identity.
    account.update_last_used();
    storage::save_account(&account).map_err(|e| anyhow::anyhow!(e))?;

    // Inject system credentials (Keychain / Credential Manager / Secret Service)
    credential::write_antigravity_system_credential(&account)
        .map_err(|e| anyhow::anyhow!("Failed to write system credentials: {}", e))?;

    google_accounts::sync_active_account(&account.email).map_err(|e| {
        anyhow::anyhow!(
            "System credentials were updated, but google_accounts.json synchronization failed: {:#}. Retry switching the account.",
            e
        )
    })?;

    // Only mark the account active after both client credential stores are synced.
    storage::set_current_account_id(&account.id).map_err(|e| {
        anyhow::anyhow!(
            "Client credentials and google_accounts.json were updated, but the local active account could not be saved: {}. Retry switching the account.",
            e
        )
    })?;

    // Optionally inject into the Antigravity IDE SQLite profile if it exists
    if let Some(db_path) = ide_db::get_default_ide_db_path() {
        let _ = ide_db::inject_account_to_ide_db(&db_path, &account);
    }

    println!(
        "{} Switched to AGY account: {}",
        "Success:".green().bold(),
        account.email.bright_green()
    );

    Ok(())
}

async fn show_current_account() -> anyhow::Result<()> {
    let current_id = storage::get_current_account_id();
    if let Some(id) = current_id {
        let account = storage::load_account(&id).map_err(|e| anyhow::anyhow!(e))?;
        println!("Currently active AGY account:");
        println!("  {:<10} {}", "Email:".bright_blue(), account.email);
        println!("  {:<10} {}", "ID:".bright_blue(), account.id);
        println!("  {:<10} {}", "Name:".bright_blue(), account.name.as_deref().unwrap_or_default());
        println!("  {:<10} {}", "Tier:".bright_blue(), tier_display(&account));
        println!("  {:<10} {}", "Tags:".bright_blue(), account.tags.join(", "));
        println!("  {:<10} {}", "Created:".bright_blue(), chrono::DateTime::from_timestamp(account.created_at, 0).unwrap_or_default().to_rfc3339());
        println!("  {:<10} {}", "Last Used:".bright_blue(), chrono::DateTime::from_timestamp(account.last_used, 0).unwrap_or_default().to_rfc3339());
    } else {
        println!("No active AGY account selected. Use `agy-auth switch <account_id_or_email>` or `agy-auth add`.");
    }
    Ok(())
}

async fn delete_account(account_ref: &str) -> anyhow::Result<()> {
    let accounts = storage::list_accounts().map_err(|e| anyhow::anyhow!(e))?;
    let target = accounts.into_iter().find(|acc| {
        acc.id == account_ref || acc.email.eq_ignore_ascii_case(account_ref)
    }).ok_or_else(|| {
        anyhow::anyhow!("Account not found matching: '{}'", account_ref)
    })?;

    storage::delete_account(&target.id).map_err(|e| anyhow::anyhow!(e))?;
    println!("{} Deleted account: {}", "Success:".green().bold(), target.email.bright_green());

    // If we deleted the active account, clear the current active selection
    let current_id = storage::get_current_account_id();
    if current_id == Some(target.id) {
        let mut index = storage::load_account_index().map_err(|e| anyhow::anyhow!(e))?;
        index.current_account_id = None;
        storage::save_account_index(&index).map_err(|e| anyhow::anyhow!(e))?;
        println!("The active account was deleted. No active account is currently selected.");
    }

    Ok(())
}
