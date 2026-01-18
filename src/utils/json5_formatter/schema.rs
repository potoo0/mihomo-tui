use std::collections::{HashMap, VecDeque};

use serde_json::Value;

/// Collect object key paths in the exact order they will be serialized by
/// `serde_json`.
///
/// Each object key is represented as a dot-separated path (e.g. `a.b.c`).
/// The returned order is deterministic and matches the iteration order of
/// `serde_json::Map`:
///
/// - With `preserve_order` enabled: insertion order (`IndexMap`)
/// - Without `preserve_order`: sorted key order (`BTreeMap`)
///
/// Example:
///
/// ```json
/// {
///   "dns": { "enable": true },
///   "log": { "level": "info" }
/// }
/// ```
///
/// Produces:
///
/// ```json
/// ["dns", "dns.enable", "log", "log.level"]
/// ```
pub fn collect_paths(value: &Value) -> VecDeque<String> {
    fn walk(v: &Value, prefix: &mut Vec<String>, out: &mut VecDeque<String>) {
        if let Value::Object(map) = v {
            for (k, v) in map {
                prefix.push(k.clone());
                out.push_back(prefix.join("."));
                walk(v, prefix, out);
                prefix.pop();
            }
        }
    }

    let mut out = VecDeque::new();
    walk(value, &mut Vec::new(), &mut out);
    out
}

/// Extract per-field comments from a JSON Schema into a `path -> comment` map.
///
/// This function is intentionally **non-validating**. It only derives human-readable
/// annotations for use in a JSON5 view (e.g., `// ...` comments).
///
/// Comment sources:
/// - `description`: appended as the first line (if present and non-empty)
/// - `enum`: appended as an additional line in the form `Allowed values: ...`
///
/// Paths use dot notation (e.g. `dns.enable`). The root schema (empty path) is ignored.
pub fn extract_comments(schema: &Value) -> HashMap<String, String> {
    fn walk(schema: &Value, prefix: &str, out: &mut HashMap<String, String>) {
        if !prefix.is_empty() {
            let desc = schema.get("description").and_then(|v| v.as_str()).filter(|v| !v.is_empty());

            let enum_values = schema.get("enum").and_then(|v| v.as_array());

            if desc.is_some() || enum_values.is_some() {
                let mut lines = Vec::new();

                if let Some(d) = desc {
                    lines.push(d.to_string());
                }

                if let Some(enums) = enum_values {
                    let joined = enums.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                    lines.push(format!("Allowed values: {}", joined));
                }

                out.insert(prefix.to_string(), lines.join(", "));
            }
        }

        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (key, value) in props {
                let path =
                    if prefix.is_empty() { key.to_string() } else { format!("{}.{}", prefix, key) };
                walk(value, &path, out);
            }
        }
    }

    let mut out = HashMap::new();
    walk(schema, "", &mut out);
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_collect_paths() {
        let data = json!({
          "dns": { "enable": true },
          "log": { "level": "info" }
        });
        let paths = collect_paths(&data);
        assert_eq!(paths, vec!["dns", "dns.enable", "log", "log.level"]);
    }

    #[test]
    fn test_flatten_schema() {
        let schema = json!({
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "title": "Core Configuration Schema",
          "type": "object",
          "properties": {
            "tun": {
              "type": "object",
              "properties": {
                "enable": {
                  "type": "boolean",
                  "description": "tun 转发开关"
                }
              }
            },
            "mode": {
              "type": "string",
              "enum": [
                "global",
                "rule",
                "direct"
              ],
              "description": "运行模式"
            }
          }
        });

        let comments = extract_comments(&schema);
        println!("comments: {:?}", comments);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments.get("tun.enable").unwrap(), "tun 转发开关");
        assert!(comments.get("mode").unwrap().contains("运行模式"));
        assert!(
            ["global", "rule", "direct"]
                .iter()
                .all(|val| comments.get("mode").unwrap().contains(val))
        );
    }
}
