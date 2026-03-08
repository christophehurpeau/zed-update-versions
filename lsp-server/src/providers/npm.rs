use async_trait::async_trait;
use serde::Deserialize;
use tower_lsp::lsp_types::{Position, Range};
use tracing::warn;

use crate::cache::VersionResult;
use crate::providers::{ParsedDependency, Provider};

/// npm provider — resolves versions from the npm registry.
pub struct NpmProvider {
    registry: String,
    dependency_keys: Vec<String>,
    http: reqwest::Client,
}

impl NpmProvider {
    pub fn new(registry: String, dependency_keys: Vec<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(format!("update-versions-lsp/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");

        Self {
            registry,
            dependency_keys,
            http,
        }
    }
}

/// Abbreviated packument response from npm registry.
#[derive(Deserialize)]
struct NpmPackument {
    #[serde(default)]
    versions: std::collections::HashMap<String, serde_json::Value>,
}

#[async_trait]
impl Provider for NpmProvider {
    fn file_patterns(&self) -> &[&str] {
        &["package.json"]
    }

    fn name(&self) -> &str {
        "npm"
    }

    fn parse_dependencies(&self, _uri: &str, content: &str) -> Vec<ParsedDependency> {
        parse_package_json(content, &self.dependency_keys)
    }

    async fn fetch_version(&self, package_name: &str) -> VersionResult {
        let encoded_name = if package_name.starts_with('@') {
            package_name.replacen('/', "%2F", 1)
        } else {
            package_name.to_string()
        };

        let url = format!("{}/{}", self.registry, encoded_name);
        let response = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.npm.install-v1+json")
            .send()
            .await;

        let response = match response {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!(
                    package = package_name,
                    status = %r.status(),
                    "npm registry returned non-success status"
                );
                return VersionResult {
                    stable_versions: Vec::new(),
                    prerelease: None,
                };
            }
            Err(e) => {
                warn!(
                    package = package_name,
                    error = %e,
                    "Failed to fetch from npm registry"
                );
                return VersionResult {
                    stable_versions: Vec::new(),
                    prerelease: None,
                };
            }
        };

        let packument: NpmPackument = match response.json().await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    package = package_name,
                    error = %e,
                    "Failed to parse npm packument"
                );
                return VersionResult {
                    stable_versions: Vec::new(),
                    prerelease: None,
                };
            }
        };

        let mut stable_vs: Vec<semver::Version> = packument
            .versions
            .keys()
            .filter_map(|v| semver::Version::parse(v).ok())
            .filter(|v| v.pre.is_empty())
            .collect();
        stable_vs.sort_unstable_by(|a, b| b.cmp(a));
        let stable_versions: Vec<String> = stable_vs.iter().map(|v| v.to_string()).collect();

        // Find the highest prerelease version from the version keys
        let prerelease = find_highest_prerelease(&packument.versions);

        VersionResult {
            stable_versions,
            prerelease,
        }
    }
}

/// Find the highest prerelease version from a map of version strings.
fn find_highest_prerelease(
    versions: &std::collections::HashMap<String, serde_json::Value>,
) -> Option<String> {
    versions
        .keys()
        .filter_map(|v| semver::Version::parse(v).ok())
        .filter(|v| !v.pre.is_empty())
        .max()
        .map(|v| v.to_string())
}

/// Parse a package.json and extract dependencies from the configured keys.
fn parse_package_json(content: &str, dependency_keys: &[String]) -> Vec<ParsedDependency> {
    let mut deps = Vec::new();

    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return deps,
    };

    let lines: Vec<&str> = content.lines().collect();

    for dep_key in dependency_keys {
        // Support nested keys like "pnpm.overrides"
        let obj = resolve_nested_key(&parsed, dep_key);
        let obj = match obj.and_then(|v| v.as_object()) {
            Some(o) => o,
            None => continue,
        };

        for (name, value) in obj {
            let version_str = match value.as_str() {
                Some(v) => v,
                None => continue,
            };

            // Skip non-version values
            if is_unsupported_specifier(version_str) {
                continue;
            }

            // Find the line and character range for the version string
            if let Some(range) = find_version_range(&lines, dep_key, name, version_str) {
                deps.push(ParsedDependency {
                    name: name.clone(),
                    version_constraint: version_str.to_string(),
                    version_range: range,
                });
            }
        }
    }

    deps
}

/// Resolve a possibly nested key like "pnpm.overrides" from a JSON value.
fn resolve_nested_key<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in key.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

/// Check if a version specifier is unsupported (git, file, workspace, etc.)
fn is_unsupported_specifier(value: &str) -> bool {
    let prefixes = [
        "file:",
        "link:",
        "workspace:",
        "github:",
        "git+",
        "git:",
        "http:",
        "https://",
    ];
    prefixes.iter().any(|p| value.starts_with(p))
        || value.starts_with("npm:")
        || value == "*"
        || value == "latest"
}

/// Find the range (line + character offsets) of a version string within the document.
/// We search for the pattern `"name": "version"` to locate the correct line.
fn find_version_range(
    lines: &[&str],
    _dep_key: &str,
    name: &str,
    version_str: &str,
) -> Option<Range> {
    // We need to find the line that contains `"name": "version_str"`
    let name_pattern = format!("\"{}\"", name);
    let version_pattern = format!("\"{}\"", version_str);

    for (line_idx, line) in lines.iter().enumerate() {
        if !line.contains(&name_pattern) {
            continue;
        }

        // Find the version string position on this line
        if let Some(ver_start) = line.find(&version_pattern) {
            // +1 to skip the opening quote
            let content_start = ver_start + 1;
            let content_end = content_start + version_str.len();

            return Some(Range {
                start: Position {
                    line: line_idx as u32,
                    character: content_start as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: content_end as u32,
                },
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_unsupported_specifier() {
        assert!(is_unsupported_specifier("file:../local-pkg"));
        assert!(is_unsupported_specifier("link:../local-pkg"));
        assert!(is_unsupported_specifier("workspace:*"));
        assert!(is_unsupported_specifier("github:user/repo"));
        assert!(is_unsupported_specifier("git+https://github.com/user/repo"));
        assert!(is_unsupported_specifier("npm:some-package"));
        assert!(is_unsupported_specifier("*"));
        assert!(is_unsupported_specifier("latest"));

        assert!(!is_unsupported_specifier("^18.2.0"));
        assert!(!is_unsupported_specifier("~1.2.3"));
        assert!(!is_unsupported_specifier(">=1.0.0"));
        assert!(!is_unsupported_specifier("1.0.0"));
    }

    #[test]
    fn test_parse_package_json_basic() {
        let content = r#"{
  "name": "my-project",
  "dependencies": {
    "react": "^18.2.0",
    "lodash": "~4.17.21"
  },
  "devDependencies": {
    "typescript": "^5.0.0"
  }
}"#;

        let keys = vec!["dependencies".to_string(), "devDependencies".to_string()];
        let deps = parse_package_json(content, &keys);

        assert_eq!(deps.len(), 3);

        let react = deps.iter().find(|d| d.name == "react").unwrap();
        assert_eq!(react.version_constraint, "^18.2.0");

        let lodash = deps.iter().find(|d| d.name == "lodash").unwrap();
        assert_eq!(lodash.version_constraint, "~4.17.21");

        let typescript = deps.iter().find(|d| d.name == "typescript").unwrap();
        assert_eq!(typescript.version_constraint, "^5.0.0");
    }

    #[test]
    fn test_parse_package_json_skips_unsupported() {
        let content = r#"{
  "dependencies": {
    "react": "^18.2.0",
    "local-pkg": "file:../local",
    "ws-pkg": "workspace:*"
  }
}"#;
        let keys = vec!["dependencies".to_string()];
        let deps = parse_package_json(content, &keys);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "react");
    }

    #[test]
    fn test_parse_package_json_scoped_package() {
        let content = r#"{
  "dependencies": {
    "@types/react": "^18.0.0",
    "@scope/pkg": "~1.0.0"
  }
}"#;
        let keys = vec!["dependencies".to_string()];
        let deps = parse_package_json(content, &keys);
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.name == "@types/react"));
        assert!(deps.iter().any(|d| d.name == "@scope/pkg"));
    }

    #[test]
    fn test_parse_package_json_version_ranges() {
        let content = r#"{
  "dependencies": {
    "react": "^18.2.0"
  }
}"#;
        let keys = vec!["dependencies".to_string()];
        let deps = parse_package_json(content, &keys);
        assert_eq!(deps.len(), 1);

        let dep = &deps[0];
        // The range should point to "^18.2.0" in the file
        assert_eq!(dep.version_range.start.line, 2);
        assert_eq!(dep.version_range.end.line, 2);
        // Check that column offsets make sense (content starts after opening quote)
        assert!(dep.version_range.start.character < dep.version_range.end.character);
    }

    #[test]
    fn test_parse_package_json_empty() {
        let content = "{}";
        let keys = vec!["dependencies".to_string()];
        let deps = parse_package_json(content, &keys);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_package_json_invalid_json() {
        let content = "not json at all";
        let keys = vec!["dependencies".to_string()];
        let deps = parse_package_json(content, &keys);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_parse_package_json_nested_key() {
        let content = r#"{
  "pnpm": {
    "overrides": {
      "lodash": "^4.17.21"
    }
  }
}"#;
        let keys = vec!["pnpm.overrides".to_string()];
        let deps = parse_package_json(content, &keys);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "lodash");
    }

    #[test]
    fn test_find_highest_prerelease() {
        let mut versions = std::collections::HashMap::new();
        versions.insert("1.0.0".to_string(), serde_json::Value::Null);
        versions.insert("2.0.0-alpha.1".to_string(), serde_json::Value::Null);
        versions.insert("2.0.0-beta.1".to_string(), serde_json::Value::Null);
        versions.insert("2.0.0-rc.1".to_string(), serde_json::Value::Null);

        let result = find_highest_prerelease(&versions);
        assert_eq!(result, Some("2.0.0-rc.1".to_string()));
    }

    #[test]
    fn test_find_highest_prerelease_none() {
        let mut versions = std::collections::HashMap::new();
        versions.insert("1.0.0".to_string(), serde_json::Value::Null);
        versions.insert("2.0.0".to_string(), serde_json::Value::Null);

        let result = find_highest_prerelease(&versions);
        assert!(result.is_none());
    }
}
