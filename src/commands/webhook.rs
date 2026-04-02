use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::error;

/// All known webhook event types (must match server).
const EVENT_TYPES: &[&str] = &[
    "tab.opened",
    "tab.charged",
    "tab.low_balance",
    "tab.closing_soon",
    "tab.closed",
    "tab.topped_up",
    "payment.completed",
    "x402.settled",
];

#[derive(Args)]
pub struct WebhookArgs {
    #[command(subcommand)]
    pub action: WebhookAction,
}

#[derive(Subcommand)]
pub enum WebhookAction {
    /// Register a webhook endpoint
    Register(WebhookRegisterArgs),
    /// List registered webhooks
    List,
    /// Delete a webhook
    Delete(WebhookDeleteArgs),
}

#[derive(Args)]
#[command(after_help = "Available events:\n  tab.opened, tab.charged, tab.low_balance, tab.closing_soon,\n  tab.closed, tab.topped_up, payment.completed, x402.settled\n\nUse --events all to subscribe to everything.")]
pub struct WebhookRegisterArgs {
    /// Webhook URL
    pub url: String,
    /// Events to subscribe to (comma-separated, or "all")
    #[arg(long, required = true)]
    pub events: String,
    /// Webhook secret for HMAC verification (auto-generated if omitted)
    #[arg(long, default_value = "whsec_default")]
    pub secret: String,
}

#[derive(Args)]
pub struct WebhookDeleteArgs {
    /// Webhook ID
    pub id: String,
}

pub async fn run(args: WebhookArgs, mut ctx: super::Context) -> Result<()> {
    super::require_init()?;

    match args.action {
        WebhookAction::Register(a) => {
            let events: Vec<&str> = if a.events == "all" {
                EVENT_TYPES.to_vec()
            } else {
                let parsed: Vec<&str> = a.events.split(',').map(|s| s.trim()).collect();
                for e in &parsed {
                    if !EVENT_TYPES.contains(e) {
                        bail!(
                            "Unknown event: {e}\nValid events: {}",
                            EVENT_TYPES.join(", ")
                        );
                    }
                }
                parsed
            };

            let body = serde_json::json!({
                "url": a.url,
                "events": events,
                "secret": a.secret,
            });
            let resp = ctx.post("/webhooks", &body).await?;
            if ctx.json {
                error::print_json(&resp);
            } else {
                let id = resp["id"].as_str().unwrap_or("?");
                error::success(&format!("Webhook registered: {id} → {}", a.url));
            }
        }
        WebhookAction::List => {
            let resp = ctx.get("/webhooks").await?;
            if ctx.json {
                error::print_json(&resp);
            } else {
                let hooks = resp.as_array();
                match hooks {
                    Some(hooks) if !hooks.is_empty() => {
                        for wh in hooks {
                            let id = wh["id"].as_str().unwrap_or("?");
                            let url = wh["url"].as_str().unwrap_or("?");
                            let events = wh["events"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            error::print_kv(&[
                                ("ID", id),
                                ("URL", url),
                                ("Events", &events),
                            ]);
                        }
                    }
                    _ => error::success("No webhooks registered"),
                }
            }
        }
        WebhookAction::Delete(a) => {
            ctx.del(&format!("/webhooks/{}", a.id)).await?;
            if ctx.json {
                error::print_json(&serde_json::json!({ "deleted": a.id }));
            } else {
                error::success(&format!("Webhook deleted: {}", a.id));
            }
        }
    }
    Ok(())
}
