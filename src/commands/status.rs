use anyhow::Result;
use clap::Args;

use crate::error;

#[derive(Args)]
pub struct StatusArgs {
    /// Wallet address to check
    #[arg(long)]
    pub wallet: Option<String>,
}

pub async fn run(args: StatusArgs, mut ctx: super::Context) -> Result<()> {
    super::require_init()?;

    let wallet = args.wallet.unwrap_or_default();
    let path = format!("/status?wallet={wallet}");
    let resp = ctx.get(&path).await?;

    let network = if ctx.config.is_testnet() {
        "testnet"
    } else {
        "mainnet"
    };

    if ctx.json {
        let mut obj = resp.as_object().cloned().unwrap_or_default();
        obj.insert("network".to_string(), serde_json::json!(network));
        error::print_json(&serde_json::Value::Object(obj));
    } else {
        let balance = resp["balance_usdc"].as_str().unwrap_or("unknown");
        let tabs = resp["open_tabs"].as_i64().unwrap_or(0);
        let locked = resp["total_locked"].as_u64().unwrap_or(0);
        error::print_kv(&[
            ("Network", ctx.config.network_name()),
            ("Balance", &format!("{balance} USDC")),
            ("Open tabs", &tabs.to_string()),
            ("Locked", &super::format_amount(locked)),
        ]);
    }
    Ok(())
}
