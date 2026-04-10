use anyhow::Result;
use clap::Args;

use crate::error;

#[derive(Args)]
#[command(
    long_about = "Send a one-shot USDC payment. Minimum $1.00. \
        Fee: 1% (deducted from recipient). Uses EIP-2612 permits for gas-efficient approval.",
    after_long_help = "EXAMPLES:\n  \
        pay direct 0xABC...DEF 5.00\n  \
        pay direct 0xABC...DEF 25.00 --memo \"Invoice #42\""
)]
pub struct DirectArgs {
    /// Recipient wallet address (0x...)
    pub to: String,
    /// Amount in USDC (e.g., "5.00" for $5)
    pub amount: String,
    /// Optional memo
    #[arg(long)]
    pub memo: Option<String>,
}

pub async fn run(args: DirectArgs, mut ctx: super::Context) -> Result<()> {
    super::require_init()?;
    super::validate_address(&args.to)?;
    let amount = super::parse_amount(&args.amount)?;
    if amount < 1_000_000 {
        anyhow::bail!("Minimum direct payment is $1.00");
    }

    // Fetch contract addresses to determine the spender (PayDirect contract)
    let contracts = crate::permit::get_contracts(&mut ctx).await?;
    if contracts.direct.is_empty() {
        anyhow::bail!("PayDirect contract address not available from server");
    }

    // Sign EIP-2612 permit for USDC approval
    let permit = crate::permit::prepare_and_sign(&mut ctx, amount, &contracts.direct).await?;

    let memo_hex = args
        .memo
        .as_deref()
        .unwrap_or("")
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    let body = serde_json::json!({
        "to": args.to,
        "amount": amount,
        "memo": memo_hex,
        "permit": permit.to_json(),
    });

    let resp = ctx.post("/direct", &body).await?;

    if ctx.json {
        error::print_json(&resp);
    } else {
        let tx = resp["tx_hash"].as_str().unwrap_or("pending");
        let status = resp["status"].as_str().unwrap_or("unknown");
        error::success(&format!(
            "Sent {} to {} [{}] tx: {}",
            super::format_amount(amount),
            args.to,
            status,
            tx,
        ));
    }
    Ok(())
}
