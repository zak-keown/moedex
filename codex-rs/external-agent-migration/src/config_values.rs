use std::fs;
use std::io;
use std::path::Path;
use toml::Value as TomlValue;

use crate::utils::invalid_data_error;

pub(crate) fn sanitized_codex_config(raw: &str) -> io::Result<(Vec<u8>, Vec<String>)> {
    let _: codex_config::config_toml::ConfigToml = toml::from_str(raw)
        .map_err(|_| invalid_data_error("Codex config contains unsupported settings"))?;
    let mut value: TomlValue =
        toml::from_str(raw).map_err(|_| invalid_data_error("Codex config is not valid TOML"))?;
    let mut review_reasons = Vec::new();
    redact_secret_bearing_config(&mut value, &mut review_reasons);
    sanitize_codex_value(&mut value, &mut Vec::new(), &mut review_reasons);
    let rendered =
        toml::to_string_pretty(&value).map_err(|err| invalid_data_error(err.to_string()))?;
    Ok((rendered.into_bytes(), review_reasons))
}

fn redact_secret_bearing_config(value: &mut TomlValue, review_reasons: &mut Vec<String>) {
    let Some(root) = value.as_table_mut() else {
        return;
    };
    for key in ["shell_environment_policy", "projects"] {
        if root.remove(key).is_some() {
            review_reasons.push(format!("{key}: omitted from credential-free import"));
        }
    }
    if let Some(servers) = root
        .get_mut("mcp_servers")
        .and_then(TomlValue::as_table_mut)
    {
        for server in servers
            .iter_mut()
            .filter_map(|(_, value)| value.as_table_mut())
        {
            for key in [
                "env",
                "bearer_token_env_var",
                "http_headers",
                "env_http_headers",
                "http_headers_helper",
            ] {
                if server.remove(key).is_some() {
                    review_reasons.push(format!(
                        "mcp_servers.*.{key}: omitted from credential-free import"
                    ));
                }
            }
        }
    }
    if let Some(providers) = root
        .get_mut("model_providers")
        .and_then(TomlValue::as_table_mut)
    {
        for provider in providers
            .iter_mut()
            .filter_map(|(_, value)| value.as_table_mut())
        {
            for key in [
                "experimental_bearer_token",
                "query_params",
                "http_headers",
                "env_http_headers",
                "auth",
                "aws",
            ] {
                if provider.remove(key).is_some() {
                    review_reasons.push(format!(
                        "model_providers.*.{key}: omitted from credential-free import"
                    ));
                }
            }
        }
    }
}

fn sanitize_codex_value(
    value: &mut TomlValue,
    path: &mut Vec<String>,
    review_reasons: &mut Vec<String>,
) {
    let TomlValue::Table(table) = value else {
        return;
    };
    let keys = table.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        path.push(key.clone());
        let dotted = path.join(".");
        let key_lower = key.to_ascii_lowercase();
        let remove_for_trust = path.first().is_some_and(|root| root == "projects");
        let remove_for_secret = ["secret", "token", "password", "api_key", "apikey"]
            .iter()
            .any(|fragment| key_lower.contains(fragment));
        let remove_for_execution = ["hook", "notify", "command", "executable"]
            .iter()
            .any(|fragment| key_lower.contains(fragment));
        let remove_for_path = table.get(&key).is_some_and(contains_absolute_path);
        if remove_for_trust || remove_for_secret || remove_for_execution || remove_for_path {
            table.remove(&key);
            let reason = if remove_for_trust {
                "project trust"
            } else if remove_for_secret {
                "sensitive value"
            } else if remove_for_execution {
                "executable or hook setting"
            } else {
                "home-bound or absolute path"
            };
            review_reasons.push(if remove_for_secret {
                "sensitive value omitted from import; review the source config manually".to_string()
            } else {
                format!("{dotted}: {reason} requires manual review")
            });
        } else if let Some(child) = table.get_mut(&key) {
            sanitize_codex_value(child, path, review_reasons);
        }
        path.pop();
    }
}

fn contains_absolute_path(value: &TomlValue) -> bool {
    match value {
        TomlValue::String(value) => Path::new(value).is_absolute(),
        TomlValue::Array(values) => values.iter().any(contains_absolute_path),
        TomlValue::Table(table) => table.values().any(contains_absolute_path),
        TomlValue::Integer(_)
        | TomlValue::Float(_)
        | TomlValue::Boolean(_)
        | TomlValue::Datetime(_) => false,
    }
}

pub(super) fn merge_missing_toml_values(
    existing: &mut TomlValue,
    incoming: &TomlValue,
) -> io::Result<bool> {
    match (existing, incoming) {
        (TomlValue::Table(existing_table), TomlValue::Table(incoming_table)) => {
            let mut changed = false;
            for (key, incoming_value) in incoming_table {
                match existing_table.get_mut(key) {
                    Some(existing_value) => {
                        if matches!(
                            (&*existing_value, incoming_value),
                            (TomlValue::Table(_), TomlValue::Table(_))
                        ) && merge_missing_toml_values(existing_value, incoming_value)?
                        {
                            changed = true;
                        }
                    }
                    None => {
                        existing_table.insert(key.clone(), incoming_value.clone());
                        changed = true;
                    }
                }
            }
            Ok(changed)
        }
        _ => Err(invalid_data_error(
            "expected TOML table while merging migrated config values",
        )),
    }
}

pub(super) fn merge_missing_mcp_servers(
    existing: &mut TomlValue,
    incoming: &TomlValue,
) -> io::Result<Vec<String>> {
    let existing_root = existing
        .as_table_mut()
        .ok_or_else(|| invalid_data_error("expected existing config to be a TOML table"))?;
    let incoming_root = incoming
        .as_table()
        .ok_or_else(|| invalid_data_error("expected migrated MCP config to be a TOML table"))?;
    let Some(incoming_servers) = incoming_root.get("mcp_servers") else {
        return Ok(Vec::new());
    };
    let incoming_servers = incoming_servers
        .as_table()
        .ok_or_else(|| invalid_data_error("expected migrated MCP servers to be a TOML table"))?;
    let Some(existing_servers) = existing_root.get_mut("mcp_servers") else {
        existing_root.insert(
            "mcp_servers".to_string(),
            TomlValue::Table(incoming_servers.clone()),
        );
        return Ok(incoming_servers.keys().cloned().collect());
    };
    let Some(existing_servers) = existing_servers.as_table_mut() else {
        return Ok(Vec::new());
    };

    let mut merged_server_names = Vec::new();
    for (server_name, incoming_server) in incoming_servers {
        if !existing_servers.contains_key(server_name) {
            existing_servers.insert(server_name.clone(), incoming_server.clone());
            merged_server_names.push(server_name.clone());
        }
    }
    Ok(merged_server_names)
}

pub(super) fn write_toml_file(path: &Path, value: &TomlValue) -> io::Result<()> {
    let serialized = toml::to_string_pretty(value)
        .map_err(|err| invalid_data_error(format!("failed to serialize config.toml: {err}")))?;
    fs::write(path, format!("{}\n", serialized.trim_end()))
}

pub(super) fn migrated_mcp_server_names(value: &TomlValue) -> Vec<String> {
    value
        .get("mcp_servers")
        .and_then(TomlValue::as_table)
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn is_empty_toml_table(value: &TomlValue) -> bool {
    match value {
        TomlValue::Table(table) => table.is_empty(),
        TomlValue::String(_)
        | TomlValue::Integer(_)
        | TomlValue::Float(_)
        | TomlValue::Boolean(_)
        | TomlValue::Datetime(_)
        | TomlValue::Array(_) => false,
    }
}
