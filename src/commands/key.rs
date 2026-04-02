//! `pay key` subcommand — plain private key management.
//!
//! Generates a raw secp256k1 keypair without encrypted storage.
//! For dev/testing only. Use `pay init` (default signer) for production.

use anyhow::Result;
use clap::Subcommand;

use crate::commands::Context;
use crate::error;

#[derive(Subcommand)]
pub enum KeyAction {
    /// Generate a raw private key (dev/testing only)
    Init(KeyInitArgs),
}

#[derive(clap::Args)]
pub struct KeyInitArgs {
    /// Write PAYSKILL_KEY to .env file
    #[arg(long)]
    pub write_env: bool,
}

pub async fn run(action: KeyAction, ctx: Context) -> Result<()> {
    match action {
        KeyAction::Init(args) => run_init(args, ctx).await,
    }
}

async fn run_init(args: KeyInitArgs, ctx: Context) -> Result<()> {
    use k256::ecdsa::SigningKey;

    let mut rng_bytes = [0u8; 32];
    getrandom::fill(&mut rng_bytes).map_err(|e| anyhow::anyhow!("rng failed: {e}"))?;
    let signing_key = SigningKey::from_bytes((&rng_bytes).into())
        .map_err(|e| anyhow::anyhow!("key generation failed: {e}"))?;

    let address = crate::auth::derive_address(&signing_key);
    let key_bytes = signing_key.to_bytes();
    let private_key = format!("0x{}", hex::encode(key_bytes));

    if args.write_env {
        let env_path = std::path::Path::new(".env");
        let line = format!("PAYSKILL_KEY={private_key}\n");

        if env_path.exists() {
            let contents = std::fs::read_to_string(env_path)
                .map_err(|e| anyhow::anyhow!("failed to read .env: {e}"))?;

            let has_key = contents
                .lines()
                .any(|l| l.starts_with("PAYSKILL_KEY=") || l.starts_with("PAYSKILL_KEY ="));

            if has_key {
                let new_contents: String = contents
                    .lines()
                    .map(|l| {
                        if l.starts_with("PAYSKILL_KEY=") || l.starts_with("PAYSKILL_KEY =") {
                            format!("PAYSKILL_KEY={private_key}")
                        } else {
                            l.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                std::fs::write(env_path, new_contents)
                    .map_err(|e| anyhow::anyhow!("failed to write .env: {e}"))?;
                error::success("Replaced PAYSKILL_KEY in .env");
            } else {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(env_path)
                    .map_err(|e| anyhow::anyhow!("failed to open .env: {e}"))?;
                file.write_all(line.as_bytes())
                    .map_err(|e| anyhow::anyhow!("failed to write to .env: {e}"))?;
                error::success("Appended PAYSKILL_KEY to .env");
            }
        } else {
            std::fs::write(env_path, &line)
                .map_err(|e| anyhow::anyhow!("failed to create .env: {e}"))?;
            error::success("Created .env with PAYSKILL_KEY");
        }
    }

    if ctx.json {
        error::print_json(&serde_json::json!({
            "address": address,
            "private_key": private_key,
        }));
    } else {
        error::print_kv(&[
            ("Address", address.as_str()),
            ("Private key", private_key.as_str()),
        ]);
        println!();
        println!("Back up your private key. It cannot be recovered if lost.");
        println!("  Set it in your environment:");
        println!("  export PAYSKILL_KEY={private_key}");
        if !args.write_env {
            println!();
            println!("  Or run with --write-env to write it to .env automatically.");
        }
        println!();
        println!("  For better security, use `pay init` for the default encrypted signer.");
    }

    Ok(())
}
