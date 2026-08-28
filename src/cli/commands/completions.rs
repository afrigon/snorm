use clap::Args;
use clap::ValueEnum;

use crate::cli::CommandHandler;
use crate::context::SnormContext;
use crate::utils::errors::CliResult;

#[derive(Args)]
pub struct CompletionsCommand {
    /// Shell to print the registration line for
    pub shell: Shell
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh
}

impl CommandHandler for CompletionsCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let (line, target) = match self.shell {
            Shell::Bash => ("source <(COMPLETE=bash snorm)", "~/.bashrc"),
            Shell::Elvish => (
                "eval (E:COMPLETE=elvish snorm | slurp)",
                "~/.config/elvish/rc.elv"
            ),
            Shell::Fish => ("COMPLETE=fish snorm | source", "~/.config/fish/config.fish"),
            Shell::Powershell => (
                "$env:COMPLETE = \"powershell\"; snorm | Out-String | Invoke-Expression; \
                 Remove-Item Env:\\COMPLETE",
                "$PROFILE"
            ),
            Shell::Zsh => ("source <(COMPLETE=zsh snorm)", "~/.zshrc")
        };

        let mut shell = context.shell();

        writeln!(shell.out(), "{line}")?;

        drop(shell);

        context
            .shell()
            .note(format!("add this line to {target} to enable completions"))?;

        Ok(())
    }
}
