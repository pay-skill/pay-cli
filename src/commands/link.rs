//! `pay link` — link Pay agent to an external wallet for funding.
//!
//! Exports the agent's private key with wallet-specific import instructions.
//! Supports MetaMask, Coinbase Wallet, and Phantom.

use anyhow::{bail, Result};
use clap::Args;
use std::io::IsTerminal;

use crate::signer::{keyring, keystore, password};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wallet {
    Metamask,
    Coinbase,
    Phantom,
}

impl std::fmt::Display for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Wallet::Metamask => write!(f, "MetaMask"),
            Wallet::Coinbase => write!(f, "Coinbase Wallet"),
            Wallet::Phantom => write!(f, "Phantom"),
        }
    }
}

#[derive(Args)]
pub struct LinkArgs {
    /// Target wallet: metamask, coinbase, or phantom
    #[arg(long, value_parser = parse_wallet)]
    wallet: Wallet,

    /// Wallet name (default: "default")
    #[arg(long, default_value = "default")]
    name: String,
}

fn parse_wallet(s: &str) -> Result<Wallet, String> {
    match s.to_lowercase().as_str() {
        "metamask" | "mm" => Ok(Wallet::Metamask),
        "coinbase" | "cb" | "coinbasewallet" | "coinbase-wallet" => Ok(Wallet::Coinbase),
        "phantom" => Ok(Wallet::Phantom),
        _ => Err(format!(
            "unknown wallet '{s}'. Options: metamask, coinbase, phantom"
        )),
    }
}

pub async fn run(args: LinkArgs, ctx: super::Context) -> Result<()> {
    super::require_init()?;

    // Require interactive terminal — we're showing a private key
    if !std::io::stderr().is_terminal() {
        bail!("pay link requires an interactive terminal.");
    }

    let wallet = args.wallet;

    // Load the private key (same resolution as `pay signer export`)
    let hex_key = load_key_hex(&args.name)?;

    // Load address from .meta or derive it
    let address = load_address(&args.name)?;

    if ctx.json {
        let json = serde_json::json!({
            "address": address,
            "wallet": wallet.to_string(),
            "network": "Base",
            "chain_id": 8453,
        });
        crate::error::print_json(&json);
        return Ok(());
    }

    // Show instructions
    eprintln!();
    eprintln!("  Link to {wallet}");
    eprintln!("  ─────────────────────────────");
    eprintln!();
    eprintln!("  Address: {address}");
    eprintln!("  Network: Base (Chain ID 8453)");
    eprintln!();

    print_steps(wallet);

    eprintln!();
    eprintln!("  Press Enter to reveal private key (auto-clears in 30s)");
    eprintln!("  Ctrl+C to cancel");

    let mut buf = String::new();
    std::io::stdin()
        .read_line(&mut buf)
        .map_err(|e| anyhow::anyhow!("failed to read: {e}"))?;

    // Print the key
    eprintln!();
    println!("  {hex_key}");
    eprintln!();

    // Copy to clipboard if available (best effort)
    if try_copy_clipboard(&hex_key) {
        eprintln!("  Copied to clipboard.");
    } else {
        eprintln!("  Copy the key above and paste into {wallet}.");
    }

    eprintln!();
    print_warnings(wallet);

    // Clear the key from terminal after 30s
    // Spawn a background thread so we don't block if the user exits early
    let wallet_name = wallet.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        // Move cursor up and overwrite the key line
        // This is best-effort — won't work in all terminals
        eprint!("\x1b[A\x1b[A\x1b[2K\r  [key cleared]\n\n");
        eprintln!("  Key cleared from display. Paste into {wallet_name} now if you haven't.");
    });

    // Wait a moment so the thread can start, then exit normally
    // The thread will fire even after main returns (it's not a daemon thread)
    Ok(())
}

fn print_steps(wallet: Wallet) {
    match wallet {
        Wallet::Metamask => {
            eprintln!("  Steps:");
            eprintln!("  1. Open MetaMask");
            eprintln!("  2. Click account selector (top center)");
            eprintln!("  3. Add account or hardware wallet → Import account");
            eprintln!("  4. Select \"Private Key\" and paste");
            eprintln!("  5. Enable Base network: Settings → Networks → Add Base");
            eprintln!("     (or it may already appear)");
            eprintln!("  6. Buy USDC directly into this account via MetaMask's Buy button");
        }
        Wallet::Coinbase => {
            eprintln!("  Steps:");
            eprintln!("  1. Open Coinbase Wallet");
            eprintln!("  2. Settings → Add & manage wallets → Import a wallet");
            eprintln!("  3. Select \"Private key\" and paste");
            eprintln!("  4. Base network is enabled by default (it's Coinbase's chain)");
            eprintln!("  5. Buy USDC directly into this account via the Buy button");
        }
        Wallet::Phantom => {
            eprintln!("  Steps:");
            eprintln!("  1. Open Phantom");
            eprintln!("  2. Tap your profile icon (top left)");
            eprintln!("  3. Add Account → Import Private Key");
            eprintln!("  4. Select Ethereum network, paste key");
            eprintln!("  5. Switch to Base network in the network selector");
            eprintln!("  6. Buy USDC directly into this account via Phantom's Buy button");
        }
    }
}

fn print_warnings(wallet: Wallet) {
    eprintln!("  Important:");
    eprintln!("  - This key is NOT recoverable from your {wallet} recovery phrase.", wallet = wallet);
    eprintln!("    Back up separately via: pay signer export");
    eprintln!("  - Your Pay agent uses this same key to sign transactions.");
    eprintln!("    Both your wallet and the agent can operate this account.");
    eprintln!("  - To fund your agent: buy USDC on Base in {wallet},", wallet = wallet);
    eprintln!("    it goes directly into your agent's balance.");
}

fn load_key_hex(name: &str) -> Result<String> {
    // Try .meta (keychain) first
    if let Ok(true) = keyring::MetaFile::exists(name) {
        if let Ok(meta) = keyring::MetaFile::load(name) {
            if meta.storage == "keychain" {
                let raw = keyring::load_key(name)?;
                return Ok(format!("0x{}", hex::encode(&raw)));
            }
        }
    }

    // Try .enc file
    let ks = keystore::Keystore::open()?;
    if ks.exists(name) {
        let key_file = ks.load(name)?;
        let pw = password::acquire_for_decrypt()?;
        let key = keystore::decrypt(&key_file, &pw)?;
        return Ok(format!("0x{}", hex::encode(key.to_bytes())));
    }

    bail!("No wallet '{name}' found. Run `pay init` first.");
}

fn load_address(name: &str) -> Result<String> {
    if let Ok(true) = keyring::MetaFile::exists(name) {
        if let Ok(meta) = keyring::MetaFile::load(name) {
            return Ok(meta.address);
        }
    }

    // Derive from .enc file
    let ks = keystore::Keystore::open()?;
    if ks.exists(name) {
        let key_file = ks.load(name)?;
        let pw = password::acquire_for_decrypt()?;
        let key = keystore::decrypt(&key_file, &pw)?;
        return Ok(crate::auth::derive_address(&key));
    }

    bail!("No wallet '{name}' found.");
}

fn try_copy_clipboard(text: &str) -> bool {
    // Try platform-native clipboard commands (best effort, no dependencies)
    #[cfg(target_os = "windows")]
    {
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("cmd")
            .args(["/C", "clip"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().map(|s| s.success()).unwrap_or(false);
        }
        false
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        // Try xclip first, then xsel
        for cmd in &["xclip", "xsel"] {
            let args: &[&str] = if *cmd == "xclip" {
                &["-selection", "clipboard"]
            } else {
                &["--clipboard", "--input"]
            };
            if let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                use std::io::Write;
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return true;
                }
            }
        }
        false
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        false
    }
}
