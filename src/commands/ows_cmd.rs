//! `pay ows` subcommand — Open Wallet Standard integration.
//!
//! All interaction goes through the `ows` CLI binary (subprocess).
//! If OWS isn't installed, commands fail loud with install instructions.

use anyhow::Result;
use clap::Subcommand;

use crate::commands::Context;
use crate::error;
use crate::ows;

#[derive(Subcommand)]
pub enum OwsAction {
    /// Create an OWS wallet with chain-lock policy and API key
    Init(OwsInitArgs),
    /// List all OWS wallets
    List,
    /// Generate a fund link for an OWS wallet
    Fund(OwsFundArgs),
    /// Set spending policy on an OWS wallet
    SetPolicy(SetPolicyArgs),
}

#[derive(clap::Args)]
pub struct OwsInitArgs {
    /// Wallet name (default: pay-{hostname})
    #[arg(long)]
    pub name: Option<String>,

    /// Chain: "base" (mainnet) or "base-sepolia" (testnet)
    #[arg(long)]
    pub chain: Option<String>,
}

#[derive(clap::Args)]
pub struct OwsFundArgs {
    /// Wallet name or ID (default: detect from OWS_WALLET_ID env)
    #[arg(long)]
    pub wallet: Option<String>,

    /// Pre-fill amount in USDC
    #[arg(long)]
    pub amount: Option<String>,
}

#[derive(clap::Args)]
pub struct SetPolicyArgs {
    /// Chain: "base" or "base-sepolia"
    #[arg(long)]
    pub chain: Option<String>,

    /// Per-transaction spending cap in USDC (e.g., 500)
    #[arg(long, allow_hyphen_values = true)]
    pub max_tx: Option<f64>,

    /// Daily spending cap in USDC (e.g., 5000)
    #[arg(long, allow_hyphen_values = true)]
    pub daily_limit: Option<f64>,
}

pub async fn run(action: OwsAction, ctx: Context) -> Result<()> {
    match action {
        OwsAction::Init(args) => run_init(args, ctx).await,
        OwsAction::List => run_list(ctx).await,
        OwsAction::Fund(args) => run_fund(args, ctx).await,
        OwsAction::SetPolicy(args) => run_set_policy(args, ctx).await,
    }
}

/// Helper to extract a string from a JSON value.
fn jstr(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ── Init ─────────────────────────────────────────────────────────────

async fn run_init(args: OwsInitArgs, ctx: Context) -> Result<()> {
    let chain = args.chain.unwrap_or_else(ows::detect_chain);
    let wallet_name = args.name.unwrap_or_else(ows::default_wallet_name);

    ows::chain_to_caip2(&chain)?;

    // Step 1: Check if OWS is installed
    if !ows::is_ows_available() {
        println!("OWS not detected. Installing via npm...");
        ows::install_ows_via_npm()?;

        if !ows::is_ows_available() {
            return Err(anyhow::anyhow!(
                "OWS installation failed. Install manually:\n  \
                 npm install -g @open-wallet-standard/core\n  \
                 Or use `pay init` for Pay's default signer."
            ));
        }
        error::success("OWS installed");
    }

    // Step 2: Create wallet
    println!("Creating wallet '{wallet_name}'...");
    let wallet = ows::create_wallet(&wallet_name)?;
    let address = ows::wallet_evm_address(&wallet)
        .ok_or_else(|| anyhow::anyhow!("wallet has no EVM account"))?;
    error::success(&format!("Wallet created: {address}"));

    // Step 3: Create chain-lock policy
    let policy = ows::create_chain_policy(&chain)?;
    let policy_id = jstr(&policy, "id");
    error::success(&format!(
        "Policy created: {policy_id} (chain lock: {chain})"
    ));

    // Step 4: Create API key bound to wallet + policy
    let wallet_id = jstr(&wallet, "id");
    let key_result = ows::create_api_key(&wallet_id, &policy_id)?;
    let token_field = jstr(&key_result, "token");
    let token = if token_field.is_empty() {
        jstr(&key_result, "api_key")
    } else {
        token_field
    };
    error::success("API key created");

    // Step 5: Output
    if ctx.json {
        error::print_json(&serde_json::json!({
            "wallet_id": wallet_id,
            "wallet_name": jstr(&wallet, "name"),
            "address": address,
            "chain": chain,
            "policy_id": policy_id,
            "api_key": token,
            "mcp_config": serde_json::from_str::<serde_json::Value>(
                &ows::mcp_config_json(&wallet_name, &chain)
            ).unwrap_or_default(),
        }));
    } else {
        println!();
        error::print_kv(&[
            ("Wallet", &wallet_name),
            ("Address", &address),
            ("Chain", &chain),
            ("Policy", &policy_id),
            ("Vault", &ows::vault_path_display()),
        ]);
        println!();
        eprintln!("API Key (save this — shown once):");
        eprintln!("  {token}");
        println!();
        eprintln!("MCP config (add to your claude_desktop_config.json):");
        eprintln!("{}", ows::mcp_config_json(&wallet_name, &chain));
        println!();
        eprintln!("Set OWS_API_KEY={token} in your environment.");
        eprintln!("Then your agent can use Pay via MCP with OWS-secured signing.");
    }

    Ok(())
}

// ── List ─────────────────────────────────────────────────────────────

async fn run_list(ctx: Context) -> Result<()> {
    let wallets = ows::list_wallets()?;

    if wallets.is_empty() {
        if ctx.json {
            error::print_json(&serde_json::json!([]));
        } else {
            println!("No OWS wallets found. Run `pay ows init` to create one.");
        }
        return Ok(());
    }

    if ctx.json {
        let entries: Vec<serde_json::Value> = wallets
            .iter()
            .map(|w| {
                let address = ows::wallet_evm_address(w).unwrap_or_default();
                serde_json::json!({
                    "id": jstr(w, "id"),
                    "name": jstr(w, "name"),
                    "address": address,
                    "created_at": jstr(w, "createdAt"),
                })
            })
            .collect();
        error::print_json(&entries);
    } else {
        let mut table = comfy_table::Table::new();
        table.set_header(vec!["Name", "Address", "ID", "Created"]);
        for w in &wallets {
            let address = ows::wallet_evm_address(w).unwrap_or_else(|| "\u{2014}".to_string());
            let id = jstr(w, "id");
            let short_id = if id.len() > 8 {
                format!("{}...", &id[..8])
            } else {
                id
            };
            table.add_row(vec![
                jstr(w, "name"),
                address,
                short_id,
                jstr(w, "createdAt"),
            ]);
        }
        println!("{table}");
    }

    Ok(())
}

// ── Fund ─────────────────────────────────────────────────────────────

async fn run_fund(args: OwsFundArgs, ctx: Context) -> Result<()> {
    let wallet_name = args
        .wallet
        .or_else(|| std::env::var("OWS_WALLET_ID").ok())
        .ok_or_else(|| {
            anyhow::anyhow!("no wallet specified. Use --wallet or set OWS_WALLET_ID.")
        })?;

    let wallet = ows::get_wallet(&wallet_name)?;
    let address = ows::wallet_evm_address(&wallet)
        .ok_or_else(|| anyhow::anyhow!("wallet has no EVM account"))?;
    let name = jstr(&wallet, "name");

    let base = if ctx.config.is_testnet() {
        "https://testnet.pay-skill.com"
    } else {
        "https://pay-skill.com"
    };

    let mut url = format!("{base}/fund/{address}");
    if let Some(ref amount) = args.amount {
        url = format!("{url}?amount={amount}");
    }

    if ctx.json {
        error::print_json(&serde_json::json!({
            "url": url,
            "wallet": name,
            "address": address,
        }));
    } else {
        error::print_kv(&[("Wallet", name.as_str()), ("Address", &address)]);
        println!();
        error::success(&format!("Fund link: {url}"));
        println!();
        println!("Open this link to add USDC to your wallet.");

        if open_url(&url).is_err() {
            println!("(Could not open browser automatically)");
        }
    }

    Ok(())
}

// ── Set Policy ───────────────────────────────────────────────────────

async fn run_set_policy(args: SetPolicyArgs, ctx: Context) -> Result<()> {
    let chain = args.chain.unwrap_or_else(ows::detect_chain);
    ows::chain_to_caip2(&chain)?;

    let has_limits = args.max_tx.is_some() || args.daily_limit.is_some();

    let policy = if has_limits {
        if let Some(max_tx) = args.max_tx {
            if max_tx <= 0.0 {
                return Err(anyhow::anyhow!("--max-tx must be positive, got {max_tx}"));
            }
        }
        if let Some(daily) = args.daily_limit {
            if daily <= 0.0 {
                return Err(anyhow::anyhow!(
                    "--daily-limit must be positive, got {daily}"
                ));
            }
        }
        ows::create_spending_policy(&chain, args.max_tx, args.daily_limit)?
    } else {
        ows::create_chain_policy(&chain)?
    };

    let policy_id = jstr(&policy, "id");
    let policy_name = jstr(&policy, "name");

    if ctx.json {
        error::print_json(&serde_json::json!({
            "policy_id": policy_id,
            "name": policy_name,
            "chain": chain,
            "max_tx_usdc": args.max_tx,
            "daily_limit_usdc": args.daily_limit,
        }));
    } else {
        error::success(&format!("Policy '{policy_id}' saved"));
        error::print_kv(&[
            ("Policy ID", policy_id.as_str()),
            ("Name", policy_name.as_str()),
            ("Chain", &chain),
            (
                "Max per-tx",
                &args
                    .max_tx
                    .map(|v| format!("${v}"))
                    .unwrap_or_else(|| "unlimited".to_string()),
            ),
            (
                "Daily limit",
                &args
                    .daily_limit
                    .map(|v| format!("${v}"))
                    .unwrap_or_else(|| "unlimited".to_string()),
            ),
        ]);

        if has_limits {
            println!();
            println!("Spending limits use the @pay-skill/ows-policy executable.");
            println!("Install it: npm install -g @pay-skill/ows-policy");
        }
    }

    Ok(())
}

/// Try to open a URL in the default browser.
fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to open browser: {e}"))?;
    }
    Ok(())
}
