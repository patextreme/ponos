//! Typed result contracts: eager JSON Schema compilation and validation.
//!
//! A session that declares `agent:session({ resultSchema = <schema> })`
//! compiles the schema eagerly (author errors fail at the author's line,
//! and remote `$ref`s are rejected so runs stay offline). Validation
//! produces human-readable violations; everything socket- or
//! channel-shaped lives in `result_wire` instead.

/// Upper bound on violations relayed in one verdict (message quality, not
/// a flood).
const MAX_VIOLATIONS: usize = 10;

use std::sync::Arc;

/// A compiled JSON Schema contract for one session's typed results.
#[derive(Clone)]
pub struct ResultContract {
    schema: serde_json::Value,
    validator: jsonschema::Validator,
}

impl std::fmt::Debug for ResultContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultContract")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl ResultContract {
    /// Compile a schema eagerly. Fails on invalid schemas and on any
    /// non-local `$ref` (remote references would reintroduce network
    /// access; runs must stay offline).
    pub fn compile(schema: serde_json::Value) -> Result<Self, String> {
        reject_remote_refs(&schema)?;
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| format!("invalid schema: {e}"))?;
        Ok(Self { schema, validator })
    }

    /// The declared schema, as JSON.
    pub fn schema(&self) -> &serde_json::Value {
        &self.schema
    }

    /// The declared schema serialized to a JSON string (for the injected
    /// server's env).
    pub fn schema_json(&self) -> String {
        self.schema.to_string()
    }

    /// Validate a submission; `Err` carries human-readable violations
    /// ("`"score" is a required property`"-style, with instance paths).
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), Vec<String>> {
        let errors: Vec<String> = self
            .validator
            .iter_errors(value)
            .take(MAX_VIOLATIONS)
            .map(|e| {
                let path = e.instance_path();
                if path.as_str().is_empty() {
                    e.to_string()
                } else {
                    format!("{e} (at {path})")
                }
            })
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Reject `$ref` values that are not local JSON pointers (`#…`) within the
/// same document. Walks the whole schema — a remote reference anywhere
/// fails the contract at the author's line.
fn reject_remote_refs(schema: &serde_json::Value) -> Result<(), String> {
    match schema {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(reference)) = map.get("$ref")
                && !reference.starts_with('#')
            {
                return Err(format!(
                    "remote $ref {reference:?} is not allowed: result schemas must be \
                     self-contained (offline runs)"
                ));
            }
            for value in map.values() {
                reject_remote_refs(value)?;
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                reject_remote_refs(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Where accepted submissions land: the in-flight turn's slot. The closure
/// returns `true` when the submission was accepted into a live turn, and
/// `false` when no turn was in flight (a late submission to drop).
pub type SubmissionSink = Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn contract() -> ResultContract {
        ResultContract::compile(serde_json::json!({
            "type": "object",
            "properties": { "verdict": { "type": "string" }, "score": { "type": "integer" } },
            "required": ["verdict"]
        }))
        .expect("schema compiles")
    }

    #[test]
    fn compile_rejects_remote_refs() {
        let err = ResultContract::compile(serde_json::json!({
            "$ref": "https://example.com/schema.json"
        }))
        .expect_err("remote ref must fail");
        assert!(err.contains("remote $ref"), "{err}");
        assert!(err.contains("https://example.com/schema.json"), "{err}");

        let err = ResultContract::compile(serde_json::json!({
            "type": "object",
            "properties": { "nested": { "$ref": "other-schema.json" } }
        }))
        .expect_err("nested remote ref must fail");
        assert!(err.contains("other-schema.json"), "{err}");
    }

    #[test]
    fn compile_accepts_local_refs() {
        ResultContract::compile(serde_json::json!({
            "$defs": { "v": { "type": "string" } },
            "type": "object",
            "properties": { "verdict": { "$ref": "#/$defs/v" } },
            "required": ["verdict"]
        }))
        .expect("local refs are fine");
    }

    #[test]
    fn compile_rejects_invalid_schemas() {
        let err = ResultContract::compile(serde_json::json!({ "type": "objekt" }))
            .expect_err("bad type value must fail");
        assert!(!err.is_empty());
    }

    #[test]
    fn validate_names_violations_and_paths() {
        let c = contract();
        assert!(
            c.validate(&serde_json::json!({ "verdict": "approve" }))
                .is_ok()
        );
        let errors = c
            .validate(&serde_json::json!({ "score": 3 }))
            .expect_err("missing required property");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("verdict") && e.contains("required")),
            "{errors:?}"
        );
        // Nested instance paths make violations actionable.
        let c2 = ResultContract::compile(serde_json::json!({
            "type": "array",
            "items": { "type": "object", "required": ["n"] }
        }))
        .unwrap();
        let errors = c2
            .validate(&serde_json::json!([{ "n": 1 }, {}]))
            .expect_err("second item missing n");
        assert!(errors.iter().any(|e| e.contains("(at /1)")), "{errors:?}");
    }
}
