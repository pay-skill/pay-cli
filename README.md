# pay

Command-line tool for [pay](https://pay-skill.com) — payment infrastructure for AI agents. USDC on Base.

## Install

```bash
cargo install pay-cli
```

## Setup

```bash
pay init          # Create wallet + config (defaults to Base mainnet)
pay network       # Show current network
pay mint 100      # Mint testnet USDC (testnet only)
pay status        # Check balance and open tabs
pay address       # Show wallet address
```

To use testnet:

```bash
pay network testnet
```

## Commands

### Direct Payment

Send a one-shot USDC payment. $1.00 minimum.

```bash
pay direct 0xprovider... 5.00
pay direct 0xprovider... 1.50 --memo "task-42"
```

### Tab Management

Open, charge, top up, and close pre-funded metered tabs. $5.00 minimum to open.

```bash
pay tab open 0xprovider... 20.00 --max-charge 0.50
pay tab list
pay tab topup tab_abc123 10.00
pay tab close tab_abc123
pay tab withdraw tab_abc123
```

Provider-side charging:

```bash
pay tab charge tab_abc123 0.30
```

### x402 Requests

Make HTTP requests that automatically handle 402 Payment Required responses.

```bash
pay request https://api.example.com/data
```

The CLI detects 402 responses, pays via direct or tab settlement, and retries.

### Webhooks

Register endpoints to receive payment events. You must specify which events to listen to.

```bash
pay webhook register https://myapp.com/hooks --events payment.completed,tab.charged
pay webhook register https://myapp.com/hooks --events all
pay webhook list
pay webhook delete wh_abc123
```

Available events: `tab.opened`, `tab.charged`, `tab.low_balance`, `tab.closing_soon`, `tab.closed`, `tab.topped_up`, `payment.completed`, `x402.settled`

### Network

Show or switch between mainnet and testnet.

```bash
pay network              # Show current network
pay network testnet      # Switch to Base Sepolia testnet
pay network mainnet      # Switch to Base mainnet
```

### Wallet Management

Multiple signer backends are supported:

```bash
pay init                          # Default setup (OS keychain)
pay init --no-keychain            # Force encrypted file storage
pay signer init --name trading    # Named wallet
pay signer import --key 0x...    # Import existing key
pay signer export                 # Export private key (interactive)
pay key init                      # Generate a plain dev key (stdout)
```

The `pay sign` command acts as a signing subprocess for SDKs:

```bash
echo "deadbeef..." | pay sign
```

### Funding

```bash
pay fund                              # Open Coinbase Onramp funding page
pay withdraw 0xrecipient... 50.00     # Get withdrawal link
```

## Output

Output is JSON by default. Use `--no-json` for human-readable format.

## Configuration

Config file: `~/.pay/config.toml` (created by `pay init`, updated by `pay network`)

```toml
chain_id = 8453
router_address = "0x..."
api_url = "https://pay-skill.com/api/v1"
```

| Env Var | Purpose |
|---------|---------|
| `PAYSKILL_SIGNER_KEY` | Private key or password for signer |

## Command Reference

```
pay init                              First-time wallet setup
pay status                            Balance + open tabs + network
pay address                           Show wallet address
pay network [testnet|mainnet]         Show or switch network
pay direct <to> <amount>              Send USDC ($1 min)
  --memo <text>                       Optional memo
pay tab open <provider> <amount>      Open tab ($5 min)
  --max-charge <amount>               Max per-charge limit
pay tab charge <tab_id> <amount>      Charge a tab (provider-side)
pay tab close <tab_id>                Close a tab
pay tab topup <tab_id> <amount>       Add funds to open tab
pay tab withdraw <tab_id>             Withdraw charged funds (provider)
pay tab list                          List open tabs
pay request <url>                     x402 request (auto-pay)
pay webhook register <url>            Register webhook endpoint
  --events <events>                   Events to listen to (required)
pay webhook list                      List registered webhooks
pay webhook delete <id>               Remove a webhook
pay sign                              Signer subprocess (stdin/stdout)
pay signer init|import|export         Advanced wallet management
pay key init                          Generate plain dev key
pay fund                              Open funding page
pay withdraw <to> <amount>            Withdraw USDC
pay mint <amount>                     Mint testnet USDC (testnet only)
```

## License

MIT
