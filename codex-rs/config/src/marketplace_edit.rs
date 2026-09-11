use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use toml_edit::Value as TomlValue;

use crate::CONFIG_TOML_FILE;

pub struct MarketplaceConfigUpdate<'a> {
    pub source_type: &'a str,
    pub source: &'a str,
    pub ref_name: Option<&'a str>,
    pub sparse_paths: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveMarketplaceConfigOutcome {
    Removed,
    NotFound,
    NameCaseMismatch { configured_name: String },
}

pub fn record_user_marketplace(
    codex_home: &Path,
    marketplace_name: &str,
    update: &MarketplaceConfigUpdate<'_>,
) -> std::io::Result<()> {
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    let mut doc = read_or_create_document(&config_path)?;
    upsert_marketplace(&mut doc, marketplace_name, update);
    fs::create_dir_all(codex_home)?;
    fs::write(config_path, doc.to_string())
}

pub fn remove_user_marketplace(codex_home: &Path, marketplace_name: &str) -> std::io::Result<bool> {
    let outcome = remove_user_marketplace_config(codex_home, marketplace_name)?;
    Ok(outcome == RemoveMarketplaceConfigOutcome::Removed)
}

pub fn remove_user_marketplace_config(
    codex_home: &Path,
    marketplace_name: &str,
) -> std::io::Result<RemoveMarketplaceConfigOutcome> {
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    let mut doc = match fs::read_to_string(&config_path) {
        Ok(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(RemoveMarketplaceConfigOutcome::NotFound);
        }
        Err(err) => return Err(err),
    };

    let outcome = remove_marketplace(&mut doc, marketplace_name);
    if outcome != RemoveMarketplaceConfigOutcome::Removed {
        return Ok(outcome);
    }

    fs::create_dir_all(codex_home)?;
    fs::write(config_path, doc.to_string())?;
    Ok(RemoveMarketplaceConfigOutcome::Removed)
}

fn read_or_create_document(config_path: &Path) -> std::io::Result<DocumentMut> {
    match fs::read_to_string(config_path) {
        Ok(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(err) => Err(err),
    }
}

fn upsert_marketplace(
    doc: &mut DocumentMut,
    marketplace_name: &str,
    update: &MarketplaceConfigUpdate<'_>,
) {
    let root = doc.as_table_mut();
    if !root.contains_key("marketplaces") {
        root.insert("marketplaces", TomlItem::Table(new_implicit_table()));
    }

    let Some(marketplaces_item) = root.get_mut("marketplaces") else {
        return;
    };

    // Preserve whichever container form the user already has. `marketplaces`
    // may legitimately be a proper table (`[marketplaces]` / `[marketplaces.x]`)
    // or a single inline table (`marketplaces = { a = {..}, b = {..} }`, which
    // `remove_marketplace` also supports). The previous code only recognized
    // the proper-table form and overwrote an inline table with an empty one,
    // silently dropping every already-configured marketplace.
    match marketplaces_item {
        TomlItem::Table(marketplaces) => {
            let mut entry = TomlTable::new();
            entry.set_implicit(false);
            for (key, val) in marketplace_entry_fields(update) {
                entry[key] = TomlItem::Value(val);
            }
            marketplaces.insert(marketplace_name, TomlItem::Table(entry));
        }
        TomlItem::Value(value) if value.is_inline_table() => {
            let Some(marketplaces) = value.as_inline_table_mut() else {
                return;
            };
            let mut entry = toml_edit::InlineTable::new();
            for (key, val) in marketplace_entry_fields(update) {
                entry.insert(key, val);
            }
            marketplaces.insert(marketplace_name, TomlValue::InlineTable(entry));
        }
        // Any other shape (a bare string, an array, etc.) is not a marketplaces
        // table at all; replace it with a fresh table, as before.
        _ => {
            *marketplaces_item = TomlItem::Table(new_implicit_table());
            if let Some(marketplaces) = marketplaces_item.as_table_mut() {
                let mut entry = TomlTable::new();
                entry.set_implicit(false);
                for (key, val) in marketplace_entry_fields(update) {
                    entry[key] = TomlItem::Value(val);
                }
                marketplaces.insert(marketplace_name, TomlItem::Table(entry));
            }
        }
    }
}

/// The field set of a single marketplace entry, as `toml_edit::Value`s so it can
/// populate either a proper table or an inline table without duplicating the
/// field logic.
fn marketplace_entry_fields(
    update: &MarketplaceConfigUpdate<'_>,
) -> Vec<(&'static str, TomlValue)> {
    let mut fields = vec![
        (
            "source_type",
            TomlValue::from(update.source_type.to_string()),
        ),
        ("source", TomlValue::from(update.source.to_string())),
    ];
    if let Some(ref_name) = update.ref_name {
        fields.push(("ref", TomlValue::from(ref_name.to_string())));
    }
    if !update.sparse_paths.is_empty() {
        fields.push((
            "sparse_paths",
            TomlValue::Array(update.sparse_paths.iter().map(String::as_str).collect()),
        ));
    }
    fields
}

fn remove_marketplace(
    doc: &mut DocumentMut,
    marketplace_name: &str,
) -> RemoveMarketplaceConfigOutcome {
    let root = doc.as_table_mut();
    let Some(marketplaces_item) = root.get_mut("marketplaces") else {
        return RemoveMarketplaceConfigOutcome::NotFound;
    };

    let mut remove_marketplaces = false;
    let outcome = match marketplaces_item {
        TomlItem::Table(marketplaces) => {
            let outcome = if marketplaces.remove(marketplace_name).is_some() {
                RemoveMarketplaceConfigOutcome::Removed
            } else if let Some(configured_name) =
                case_mismatched_key(marketplaces.iter().map(|(key, _)| key), marketplace_name)
            {
                RemoveMarketplaceConfigOutcome::NameCaseMismatch { configured_name }
            } else {
                RemoveMarketplaceConfigOutcome::NotFound
            };
            remove_marketplaces = marketplaces.is_empty();
            outcome
        }
        TomlItem::Value(value) => {
            let Some(marketplaces) = value.as_inline_table_mut() else {
                return RemoveMarketplaceConfigOutcome::NotFound;
            };
            let outcome = if marketplaces.remove(marketplace_name).is_some() {
                RemoveMarketplaceConfigOutcome::Removed
            } else if let Some(configured_name) =
                case_mismatched_key(marketplaces.iter().map(|(key, _)| key), marketplace_name)
            {
                RemoveMarketplaceConfigOutcome::NameCaseMismatch { configured_name }
            } else {
                RemoveMarketplaceConfigOutcome::NotFound
            };
            remove_marketplaces = marketplaces.is_empty();
            outcome
        }
        _ => RemoveMarketplaceConfigOutcome::NotFound,
    };

    if outcome == RemoveMarketplaceConfigOutcome::Removed && remove_marketplaces {
        root.remove("marketplaces");
    }
    outcome
}

fn case_mismatched_key<'a>(
    mut keys: impl Iterator<Item = &'a str>,
    requested_name: &str,
) -> Option<String> {
    keys.find(|key| *key != requested_name && key.eq_ignore_ascii_case(requested_name))
        .map(str::to_string)
}

fn new_implicit_table() -> TomlTable {
    let mut table = TomlTable::new();
    table.set_implicit(true);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    #[test]
    fn record_user_marketplace_omits_runtime_update_metadata() {
        let codex_home = TempDir::new().unwrap();
        let update = MarketplaceConfigUpdate {
            source_type: "git",
            source: "https://github.com/owner/repo.git",
            ref_name: Some("main"),
            sparse_paths: &[],
        };

        record_user_marketplace(codex_home.path(), "debug", &update).unwrap();

        let config: toml::Value =
            toml::from_str(&fs::read_to_string(codex_home.path().join(CONFIG_TOML_FILE)).unwrap())
                .unwrap();
        let marketplace = config
            .get("marketplaces")
            .and_then(|marketplaces| marketplaces.get("debug"))
            .expect("marketplace declaration");
        assert_eq!(
            marketplace.get("source_type").and_then(toml::Value::as_str),
            Some("git")
        );
        assert_eq!(
            marketplace.get("source").and_then(toml::Value::as_str),
            Some("https://github.com/owner/repo.git")
        );
        assert_eq!(
            marketplace.get("ref").and_then(toml::Value::as_str),
            Some("main")
        );
        assert!(marketplace.get("last_updated").is_none());
        assert!(marketplace.get("last_revision").is_none());
    }

    #[test]
    fn remove_user_marketplace_removes_requested_entry() {
        let codex_home = TempDir::new().unwrap();
        let update = MarketplaceConfigUpdate {
            source_type: "git",
            source: "https://github.com/owner/repo.git",
            ref_name: Some("main"),
            sparse_paths: &[],
        };
        record_user_marketplace(codex_home.path(), "debug", &update).unwrap();
        record_user_marketplace(codex_home.path(), "other", &update).unwrap();

        let removed = remove_user_marketplace(codex_home.path(), "debug").unwrap();

        assert!(removed);
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(codex_home.path().join(CONFIG_TOML_FILE)).unwrap())
                .unwrap();
        let marketplaces = config
            .get("marketplaces")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(marketplaces.len(), 1);
        assert!(marketplaces.contains_key("other"));
    }

    #[test]
    fn remove_user_marketplace_returns_false_when_missing() {
        let codex_home = TempDir::new().unwrap();

        let removed = remove_user_marketplace(codex_home.path(), "debug").unwrap();

        assert!(!removed);
    }

    #[test]
    fn remove_user_marketplace_config_reports_case_mismatch() {
        let codex_home = TempDir::new().unwrap();
        let update = MarketplaceConfigUpdate {
            source_type: "git",
            source: "https://github.com/owner/repo.git",
            ref_name: Some("main"),
            sparse_paths: &[],
        };
        record_user_marketplace(codex_home.path(), "debug", &update).unwrap();

        let outcome = remove_user_marketplace_config(codex_home.path(), "Debug").unwrap();

        assert_eq!(
            outcome,
            RemoveMarketplaceConfigOutcome::NameCaseMismatch {
                configured_name: "debug".to_string()
            }
        );
    }

    #[test]
    fn record_user_marketplace_config_preserves_inline_table_entries() {
        let codex_home = TempDir::new().unwrap();
        fs::write(
            codex_home.path().join(CONFIG_TOML_FILE),
            r#"
marketplaces = {
  debug = { source_type = "git", source = "https://github.com/owner/repo.git" },
  other = { source_type = "local", source = "/tmp/marketplace" },
}
"#,
        )
        .unwrap();

        let update = MarketplaceConfigUpdate {
            source_type: "git",
            source: "https://github.com/owner/new.git",
            ref_name: None,
            sparse_paths: &[],
        };
        record_user_marketplace(codex_home.path(), "newone", &update).unwrap();

        let config: toml::Value =
            toml::from_str(&fs::read_to_string(codex_home.path().join(CONFIG_TOML_FILE)).unwrap())
                .unwrap();
        let marketplaces = config.get("marketplaces").expect("marketplaces table");
        assert!(
            marketplaces.get("debug").is_some(),
            "existing inline-table marketplace `debug` must be preserved"
        );
        assert!(
            marketplaces.get("other").is_some(),
            "existing inline-table marketplace `other` must be preserved"
        );
        assert!(
            marketplaces.get("newone").is_some(),
            "newly added marketplace must be present"
        );
    }

    #[test]
    fn remove_user_marketplace_config_removes_inline_table_entry() {
        let codex_home = TempDir::new().unwrap();
        fs::write(
            codex_home.path().join(CONFIG_TOML_FILE),
            r#"
marketplaces = {
  debug = { source_type = "git", source = "https://github.com/owner/repo.git" },
  other = { source_type = "local", source = "/tmp/marketplace" },
}
"#,
        )
        .unwrap();

        let outcome = remove_user_marketplace_config(codex_home.path(), "debug").unwrap();

        assert_eq!(outcome, RemoveMarketplaceConfigOutcome::Removed);
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(codex_home.path().join(CONFIG_TOML_FILE)).unwrap())
                .unwrap();
        let marketplaces = config
            .get("marketplaces")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(marketplaces.len(), 1);
        assert!(marketplaces.contains_key("other"));
    }
}
