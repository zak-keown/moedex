use anyhow::Context;
use clap::Args;
use clap::Subcommand;
use codex_external_agent_migration::CodexImportSelection;
use codex_external_agent_migration::ConflictPolicy;
use codex_external_agent_migration::apply_codex_import;
use codex_external_agent_migration::preview_codex_import;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ImportCommand {
    #[command(subcommand)]
    source: ImportSource,
}

#[derive(Debug, Subcommand)]
enum ImportSource {
    /// Preview or import selected data from an existing Codex home.
    Codex(CodexImportArgs),
}

#[derive(Debug, Args)]
struct CodexImportArgs {
    /// Show the immutable import plan without writing either home.
    #[arg(long)]
    dry_run: bool,

    /// Import compatible non-secret settings.
    #[arg(long)]
    settings: bool,

    /// Import validated native rollout sessions.
    #[arg(long)]
    sessions: bool,

    /// Request credential import. Unsupported backends require a fresh login.
    #[arg(long)]
    credentials: bool,

    /// Replace conflicts after retaining a destination backup.
    #[arg(long)]
    replace: bool,

    /// Existing stock Codex home. Defaults to ~/.codex.
    #[arg(long, value_name = "PATH")]
    source_home: Option<PathBuf>,

    /// Moedex destination home. Defaults to the resolved product home.
    #[arg(long, value_name = "PATH")]
    destination_home: Option<PathBuf>,
}

pub async fn run_import_command(command: ImportCommand) -> anyhow::Result<()> {
    match command.source {
        ImportSource::Codex(args) => run_codex_import(args).await,
    }
}

async fn run_codex_import(args: CodexImportArgs) -> anyhow::Result<()> {
    if !args.settings && !args.sessions && !args.credentials {
        anyhow::bail!("select at least one of --settings, --sessions, or --credentials");
    }
    let source = match args.source_home {
        Some(path) => {
            AbsolutePathBuf::relative_to_current_dir(path).context("resolve Codex source home")?
        }
        None => AbsolutePathBuf::from_absolute_path(
            dirs::home_dir()
                .context("locate user home for the default Codex source")?
                .join(".codex"),
        )?,
    };
    let destination = match args.destination_home {
        Some(path) => AbsolutePathBuf::relative_to_current_dir(path)
            .context("resolve Moedex destination home")?,
        None => codex_core::config::find_codex_home()?,
    };
    let selection = CodexImportSelection {
        settings: args.settings,
        sessions: args.sessions,
        credentials: args.credentials,
        conflict_policy: if args.replace {
            ConflictPolicy::ReplaceWithBackup
        } else {
            ConflictPolicy::Skip
        },
    };
    let preview = preview_codex_import(source, destination, selection.clone()).await?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    if args.dry_run {
        return Ok(());
    }
    let report = apply_codex_import(&preview.id, selection).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
