use std::collections::HashMap;

pub const FEEDBACK_DIAGNOSTICS_ATTACHMENT_FILENAME: &str = "codex-connectivity-diagnostics.txt";
const PROXY_ENV_VARS: &[&str] = &[
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// Redact secrets from a proxy env-var value before it is placed in a feedback
/// diagnostic (which is uploaded to Sentry). Proxy URLs commonly embed HTTP
/// Basic-Auth credentials (`http://user:pass@host`) or tokens in the query
/// string (`?token=...`); strip userinfo and any query/fragment while keeping
/// scheme://host:port/path so the diagnostic stays useful. Values that are not
/// URLs (no `://`) are redacted in full because schemeless proxy syntax can
/// still contain userinfo. Mirrors the codebase's existing URL redaction convention
/// (`login::redact_sensitive_url_parts`,
/// `doctor::redact_url_token`) for this data class.
fn redact_proxy_value(value: &str) -> String {
    // Drop query and fragment, which can carry tokens.
    let without_query = value.split(['?', '#']).next().unwrap_or(value);
    let Some(scheme_end) = without_query.find("://") else {
        return "<redacted>".to_string();
    };
    let (scheme, rest) = without_query.split_at(scheme_end + 3);
    let (authority, path) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, ""),
    };
    match authority.rfind('@') {
        Some(at) => format!("{scheme}***@{}{path}", &authority[at + 1..]),
        None => format!("{scheme}{authority}{path}"),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedbackDiagnostics {
    diagnostics: Vec<FeedbackDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackDiagnostic {
    pub headline: String,
    pub details: Vec<String>,
}

impl FeedbackDiagnostics {
    pub fn new(diagnostics: Vec<FeedbackDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn collect_from_env() -> Self {
        Self::collect_from_pairs(std::env::vars())
    }

    fn collect_from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let env = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect::<HashMap<_, _>>();
        let mut diagnostics = Vec::new();

        let proxy_details = PROXY_ENV_VARS
            .iter()
            .filter_map(|key| {
                let value = env.get(*key)?;
                Some(format!("{key} = {}", redact_proxy_value(value)))
            })
            .collect::<Vec<_>>();
        if !proxy_details.is_empty() {
            diagnostics.push(FeedbackDiagnostic {
                headline: "Proxy environment variables are set and may affect connectivity."
                    .to_string(),
                details: proxy_details,
            });
        }

        Self { diagnostics }
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[FeedbackDiagnostic] {
        &self.diagnostics
    }

    pub fn attachment_text(&self) -> Option<String> {
        if self.diagnostics.is_empty() {
            return None;
        }

        let mut lines = vec!["Connectivity diagnostics".to_string(), String::new()];
        for diagnostic in &self.diagnostics {
            lines.push(format!("- {}", diagnostic.headline));
            lines.extend(
                diagnostic
                    .details
                    .iter()
                    .map(|detail| format!("  - {detail}")),
            );
        }

        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::FeedbackDiagnostic;
    use super::FeedbackDiagnostics;

    #[test]
    fn collect_from_pairs_redacts_credentials_and_reports_attachment() {
        let diagnostics = FeedbackDiagnostics::collect_from_pairs([
            (
                "HTTPS_PROXY",
                "https://user:password@secure-proxy.example.com:443?secret=1",
            ),
            ("http_proxy", "user:password@proxy.example.com:8080"),
            ("all_proxy", "socks5h://all-proxy.example.com:1080"),
        ]);

        // Userinfo and query (which can carry credentials/tokens) are redacted;
        // scheme/host/port is preserved for URLs. Schemeless values are fully
        // redacted because they can still contain credentials. This text is
        // uploaded to Sentry, so it must not carry secrets.
        assert_eq!(
            diagnostics,
            FeedbackDiagnostics {
                diagnostics: vec![FeedbackDiagnostic {
                    headline: "Proxy environment variables are set and may affect connectivity."
                        .to_string(),
                    details: vec![
                        "http_proxy = <redacted>".to_string(),
                        "HTTPS_PROXY = https://***@secure-proxy.example.com:443".to_string(),
                        "all_proxy = socks5h://all-proxy.example.com:1080".to_string(),
                    ],
                },],
            }
        );

        assert_eq!(
            diagnostics.attachment_text(),
            Some(
                r#"Connectivity diagnostics

- Proxy environment variables are set and may affect connectivity.
  - http_proxy = <redacted>
  - HTTPS_PROXY = https://***@secure-proxy.example.com:443
  - all_proxy = socks5h://all-proxy.example.com:1080"#
                    .to_string()
            )
        );
    }

    #[test]
    fn collect_from_pairs_ignores_absent_values() {
        let diagnostics = FeedbackDiagnostics::collect_from_pairs(Vec::<(String, String)>::new());
        assert_eq!(diagnostics, FeedbackDiagnostics::default());
        assert_eq!(diagnostics.attachment_text(), None);
    }

    #[test]
    fn collect_from_pairs_redacts_non_url_values() {
        let diagnostics =
            FeedbackDiagnostics::collect_from_pairs([("HTTP_PROXY", "  proxy with spaces  ")]);

        assert_eq!(
            diagnostics,
            FeedbackDiagnostics {
                diagnostics: vec![FeedbackDiagnostic {
                    headline: "Proxy environment variables are set and may affect connectivity."
                        .to_string(),
                    details: vec!["HTTP_PROXY = <redacted>".to_string()],
                },],
            }
        );
    }

    #[test]
    fn collect_from_pairs_redacts_schemeless_credentials() {
        let proxy_value = "user:password@proxy.example.com:8080";
        let diagnostics = FeedbackDiagnostics::collect_from_pairs([("HTTP_PROXY", proxy_value)]);

        assert_eq!(
            diagnostics,
            FeedbackDiagnostics {
                diagnostics: vec![FeedbackDiagnostic {
                    headline: "Proxy environment variables are set and may affect connectivity."
                        .to_string(),
                    details: vec!["HTTP_PROXY = <redacted>".to_string()],
                },],
            }
        );
    }

    #[test]
    fn redact_proxy_value_strips_userinfo_and_query() {
        use super::redact_proxy_value;

        assert_eq!(
            redact_proxy_value("https://user:password@secure-proxy.example.com:443?secret=1"),
            "https://***@secure-proxy.example.com:443"
        );
        assert_eq!(
            redact_proxy_value("http://user:pass@proxy.example.com:8080/path?token=abc"),
            "http://***@proxy.example.com:8080/path"
        );
        // No userinfo: scheme/host/port kept, query dropped.
        assert_eq!(
            redact_proxy_value("socks5h://all-proxy.example.com:1080?x=1"),
            "socks5h://all-proxy.example.com:1080"
        );
        // No scheme: fully redacted because schemeless proxy syntax can contain userinfo.
        assert_eq!(redact_proxy_value("proxy.example.com:8080"), "<redacted>");
    }
}
