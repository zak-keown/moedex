use super::ImportCommand;
use clap::Parser;

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: TestCommand,
}

#[derive(Debug, clap::Subcommand)]
enum TestCommand {
    Import(ImportCommand),
}

#[test]
fn codex_import_help_exposes_safe_selection_controls() {
    let error = TestCli::try_parse_from(["moedex", "import", "codex", "--help"])
        .expect_err("help exits through clap");
    let help = error.to_string();
    for flag in [
        "--dry-run",
        "--settings",
        "--sessions",
        "--credentials",
        "--replace",
    ] {
        assert!(help.contains(flag), "missing {flag} from {help}");
    }
}

#[test]
fn codex_import_does_not_select_credentials_implicitly() {
    let cli =
        TestCli::try_parse_from(["moedex", "import", "codex", "--settings"]).expect("parse import");
    let rendered = format!("{:?}", cli.command);

    assert!(rendered.contains("settings: true"));
    assert!(rendered.contains("credentials: false"));
}
