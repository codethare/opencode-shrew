use anyhow::Result;
use clap::CommandFactory;

pub fn cmd_completion(shell: &str) -> Result<()> {
    let shell: clap_complete::Shell = shell
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid shell: '{shell}'. Use bash, zsh, fish, powershell, or elvish."))?;
    let mut cmd = crate::Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
