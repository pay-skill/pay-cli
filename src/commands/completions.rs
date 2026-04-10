use anyhow::{bail, Result};
use clap::Args;
use clap_complete::{generate, Shell};

/// Generate shell completion scripts
#[derive(Args)]
#[command(
    long_about = "Generate shell completion scripts for pay. Output the script to stdout \
        and source it in your shell profile.\n\n\
        Bash:  pay completions bash > ~/.bash_completion.d/pay\n\
        Zsh:   pay completions zsh > ~/.zfunc/_pay\n\
        Fish:  pay completions fish > ~/.config/fish/completions/pay.fish"
)]
pub struct CompletionsArgs {
    /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
    pub shell: String,
}

pub fn run(args: CompletionsArgs) -> Result<()> {
    let shell = match args.shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        other => bail!("Unknown shell: {other}. Supported: bash, zsh, fish, powershell, elvish"),
    };

    let mut cmd = crate::build_cli();
    generate(shell, &mut cmd, "pay", &mut std::io::stdout());
    Ok(())
}
