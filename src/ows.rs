//! OWS (Open Wallet Standard) integration helpers.
//!
//! All OWS interaction goes through the `ows` CLI binary (subprocess).
//! Nothing is compiled in — if OWS isn't installed, we detect that and
//! give clear instructions. Pay's own signer is always priority #1.

use anyhow::{anyhow, bail, Context, Result};

// ── Chain helpers ────────────────────────────────────────────────────

/// Map a Pay chain name to its CAIP-2 identifier.
pub fn chain_to_caip2(chain: &str) -> Result<String> {
    match chain {
        "base" => Ok("eip155:8453".to_string()),
        "base-sepolia" => Ok("eip155:84532".to_string()),
        _ => Err(anyhow!(
            "unknown chain: {chain}. Use 'base' or 'base-sepolia'."
        )),
    }
}

/// Map a Pay chain name to a numeric chain ID.
#[allow(dead_code)]
pub fn chain_to_id(chain: &str) -> Result<u64> {
    match chain {
        "base" => Ok(8453),
        "base-sepolia" => Ok(84532),
        _ => Err(anyhow!(
            "unknown chain: {chain}. Use 'base' or 'base-sepolia'."
        )),
    }
}

/// Detect chain from env var or default to "base".
pub fn detect_chain() -> String {
    std::env::var("PAYSKILL_CHAIN").unwrap_or_else(|_| "base".to_string())
}

/// Generate a wallet name from the hostname: `pay-{hostname}`.
pub fn default_wallet_name() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "agent".to_string());
    format!("pay-{host}")
}

// ── OWS CLI subprocess helpers ──────────────────────────────────────

/// Run an `ows` CLI command and return stdout as a string.
/// Fails loud if ows is not installed or the command fails.
fn run_ows(args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("ows")
        .args(args)
        .output()
        .context(
            "failed to run `ows` command. Is OWS installed?\n  \
             Install: npm install -g @open-wallet-standard/core\n  \
             Or: curl -fsSL https://openwallet.sh/install.sh | bash",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ows {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run an `ows` CLI command and parse stdout as JSON.
fn run_ows_json(args: &[&str]) -> Result<serde_json::Value> {
    let stdout = run_ows(args)?;
    serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse ows output as JSON: {stdout}"))
}

// ── Availability ────────────────────────────────────────────────────

/// Check if OWS CLI is installed by running `ows --version`.
pub fn is_ows_available() -> bool {
    std::process::Command::new("ows")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Get OWS CLI version string, if installed.
#[allow(dead_code)]
pub fn ows_cli_version() -> Option<String> {
    std::process::Command::new("ows")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Install OWS globally via npm (silent).
pub fn install_ows_via_npm() -> Result<()> {
    let status = std::process::Command::new("npm")
        .args(["install", "-g", "@open-wallet-standard/core"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to run npm install")?;

    if !status.success() {
        bail!(
            "npm install -g @open-wallet-standard/core failed (exit code: {})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

// ── Wallet operations (subprocess) ──────────────────────────────────

/// Create a wallet via `ows wallet create`.
pub fn create_wallet(name: &str) -> Result<serde_json::Value> {
    run_ows_json(&["wallet", "create", "--name", name])
}

/// Get a wallet by name or ID via `ows wallet list` + filter.
pub fn get_wallet(name_or_id: &str) -> Result<serde_json::Value> {
    let wallets = list_wallets()?;
    wallets
        .iter()
        .find(|w| {
            w.get("name").and_then(|v| v.as_str()) == Some(name_or_id)
                || w.get("id").and_then(|v| v.as_str()) == Some(name_or_id)
        })
        .cloned()
        .ok_or_else(|| anyhow!("OWS wallet not found: {name_or_id}"))
}

/// List all OWS wallets via `ows wallet list`.
pub fn list_wallets() -> Result<Vec<serde_json::Value>> {
    let output = run_ows_json(&["wallet", "list"])?;
    match output {
        serde_json::Value::Array(arr) => Ok(arr),
        _ => Ok(vec![output]),
    }
}

/// Get the EVM address from a wallet JSON object.
pub fn wallet_evm_address(wallet: &serde_json::Value) -> Option<String> {
    let accounts = wallet.get("accounts")?.as_array()?;
    accounts
        .iter()
        .find(|a| {
            let chain = a
                .get("chainId")
                .or_else(|| a.get("chain_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            chain.starts_with("eip155:") || chain == "evm"
        })
        .and_then(|a| a.get("address").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

// ── Policy operations (subprocess) ──────────────────────────────────

/// Create a chain-lock policy via `ows policy create`.
pub fn create_chain_policy(chain: &str) -> Result<serde_json::Value> {
    let caip2 = chain_to_caip2(chain)?;
    let id = format!("pay-{chain}");

    // Build policy JSON and pass via stdin or temp file
    let policy = serde_json::json!({
        "id": id,
        "name": format!("Pay {chain} chain lock"),
        "rules": [{ "type": "allowed_chains", "chain_ids": [caip2] }],
        "action": "deny"
    });

    let policy_str = serde_json::to_string(&policy)?;

    let output = std::process::Command::new("ows")
        .args(["policy", "create", "--json", &policy_str])
        .output()
        .context("failed to run ows policy create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ows policy create failed: {}", stderr.trim());
    }

    Ok(policy)
}

/// Create a spending policy with chain lock + limits.
pub fn create_spending_policy(
    chain: &str,
    max_tx_usdc: Option<f64>,
    daily_limit_usdc: Option<f64>,
) -> Result<serde_json::Value> {
    let caip2 = chain_to_caip2(chain)?;
    let id = format!("pay-{chain}-limits");

    let mut config = serde_json::Map::new();
    config.insert("chain_ids".to_string(), serde_json::json!([&caip2]));
    if let Some(max_tx) = max_tx_usdc {
        config.insert("max_tx_usdc".to_string(), serde_json::json!(max_tx));
    }
    if let Some(daily) = daily_limit_usdc {
        config.insert("daily_limit_usdc".to_string(), serde_json::json!(daily));
    }

    let policy = serde_json::json!({
        "id": id,
        "name": format!("Pay {chain} spending policy"),
        "rules": [{ "type": "allowed_chains", "chain_ids": [caip2] }],
        "executable": "npx @pay-skill/ows-policy",
        "config": config,
        "action": "deny"
    });

    let policy_str = serde_json::to_string(&policy)?;

    let output = std::process::Command::new("ows")
        .args(["policy", "create", "--json", &policy_str])
        .output()
        .context("failed to run ows policy create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ows policy create failed: {}", stderr.trim());
    }

    Ok(policy)
}

// ── API key operations (subprocess) ─────────────────────────────────

/// Create an API key bound to a wallet and policy.
/// Returns the parsed JSON output (includes the token shown once).
pub fn create_api_key(wallet_id: &str, policy_id: &str) -> Result<serde_json::Value> {
    run_ows_json(&[
        "key",
        "create",
        "--wallet",
        wallet_id,
        "--policy",
        policy_id,
        "--name",
        "pay-agent",
    ])
}

// ── Display helpers ─────────────────────────────────────────────────

/// Generate the MCP config JSON for the user to add to their config.
pub fn mcp_config_json(wallet_name: &str, chain: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "pay": {
                "command": "npx",
                "args": ["@pay-skill/mcp-server"],
                "env": {
                    "OWS_WALLET_ID": wallet_name,
                    "OWS_API_KEY": "$OWS_API_KEY",
                    "PAYSKILL_CHAIN": chain,
                }
            }
        }
    }))
    .expect("JSON serialization cannot fail for static structure")
}

/// Resolve the vault path for display purposes.
pub fn vault_path_display() -> String {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".ows").display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Chain mapping tests ──────────────────────────────────────────────

    #[test]
    fn test_chain_to_caip2_base() {
        assert_eq!(chain_to_caip2("base").unwrap(), "eip155:8453");
    }

    #[test]
    fn test_chain_to_caip2_base_sepolia() {
        assert_eq!(chain_to_caip2("base-sepolia").unwrap(), "eip155:84532");
    }

    #[test]
    fn test_chain_to_caip2_unknown() {
        assert!(chain_to_caip2("ethereum").is_err());
        assert!(chain_to_caip2("").is_err());
        assert!(chain_to_caip2("arbitrum").is_err());
    }

    #[test]
    fn test_chain_to_id_base() {
        assert_eq!(chain_to_id("base").unwrap(), 8453);
    }

    #[test]
    fn test_chain_to_id_base_sepolia() {
        assert_eq!(chain_to_id("base-sepolia").unwrap(), 84532);
    }

    #[test]
    fn test_chain_to_id_unknown() {
        assert!(chain_to_id("solana").is_err());
    }

    // ── Wallet name tests ────────────────────────────────────────────────

    #[test]
    fn test_default_wallet_name_has_prefix() {
        let name = default_wallet_name();
        assert!(
            name.starts_with("pay-"),
            "wallet name must start with 'pay-', got: {name}"
        );
        assert!(name.len() > 4, "wallet name must include hostname");
    }

    // ── MCP config tests ─────────────────────────────────────────────────

    #[test]
    fn test_mcp_config_json_structure() {
        let json = mcp_config_json("pay-test", "base");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let server = &parsed["mcpServers"]["pay"];
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"][0], "@pay-skill/mcp-server");
        assert_eq!(server["env"]["OWS_WALLET_ID"], "pay-test");
        assert_eq!(server["env"]["PAYSKILL_CHAIN"], "base");
        assert_eq!(server["env"]["OWS_API_KEY"], "$OWS_API_KEY");
    }

    #[test]
    fn test_mcp_config_json_testnet() {
        let json = mcp_config_json("pay-agent", "base-sepolia");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["mcpServers"]["pay"]["env"]["PAYSKILL_CHAIN"],
            "base-sepolia"
        );
    }

    // ── Vault path test ──────────────────────────────────────────────────

    #[test]
    fn test_vault_path_display_ends_with_ows() {
        let path = vault_path_display();
        assert!(
            path.contains(".ows"),
            "vault path must contain .ows, got: {path}"
        );
    }

    // ── EVM address extraction ───────────────────────────────────────────

    #[test]
    fn test_wallet_evm_address_found() {
        let wallet = serde_json::json!({
            "id": "test-id",
            "name": "test-wallet",
            "accounts": [{
                "chainId": "eip155:8453",
                "address": "0xdeadbeef",
                "derivationPath": "m/44'/60'/0'/0/0"
            }]
        });
        assert_eq!(wallet_evm_address(&wallet), Some("0xdeadbeef".to_string()));
    }

    #[test]
    fn test_wallet_evm_address_evm_chain_id() {
        let wallet = serde_json::json!({
            "id": "test-id",
            "name": "test-wallet",
            "accounts": [{ "chainId": "evm", "address": "0xcafe" }]
        });
        assert_eq!(wallet_evm_address(&wallet), Some("0xcafe".to_string()));
    }

    #[test]
    fn test_wallet_evm_address_snake_case_key() {
        // OWS may return snake_case depending on version
        let wallet = serde_json::json!({
            "id": "test-id",
            "name": "test-wallet",
            "accounts": [{ "chain_id": "eip155:8453", "address": "0xbeef" }]
        });
        assert_eq!(wallet_evm_address(&wallet), Some("0xbeef".to_string()));
    }

    #[test]
    fn test_wallet_evm_address_not_found() {
        let wallet = serde_json::json!({
            "id": "test-id",
            "name": "test-wallet",
            "accounts": [{ "chainId": "solana", "address": "SolAddr123" }]
        });
        assert_eq!(wallet_evm_address(&wallet), None);
    }

    #[test]
    fn test_wallet_evm_address_empty_accounts() {
        let wallet = serde_json::json!({
            "id": "test-id",
            "name": "test-wallet",
            "accounts": []
        });
        assert_eq!(wallet_evm_address(&wallet), None);
    }

    // ── Integration tests (skip if OWS not available) ────────────────────

    #[test]
    fn test_ows_availability_does_not_panic() {
        let _ = is_ows_available();
    }
}
