//! JSONPath (RFC 9535) for rules: locate/validate on top of serde_json_path.
//! One parser serves the stdlib runtime, validation on save,
//! dry-run, and hints in the editor.

use serde_json::json;
use serde_json_path::JsonPath;

/// The leading `$` is optional for ergonomics: `items[*]` → `$.items[*]`.
pub fn normalize(path: &str) -> String {
    let p = path.trim();
    if p.starts_with('$') {
        p.to_string()
    } else if p.starts_with('[') {
        format!("${p}")
    } else {
        format!("$.{p}")
    }
}

/// Match locations as JSON Pointers: {"locations":["/items/0/price"]} | {"error":"…"}.
pub fn locate(doc_json: &str, path: &str) -> String {
    let doc: serde_json::Value = match serde_json::from_str(doc_json) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("body is not JSON: {e}") }).to_string(),
    };
    let jp = match JsonPath::parse(&normalize(path)) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("JSONPath syntax error: {e}") }).to_string(),
    };
    let ptrs: Vec<String> = jp.query_located(&doc).locations().map(|l| l.to_json_pointer()).collect();
    json!({ "locations": ptrs }).to_string()
}

/// None — path is valid; Some(msg) — parser error text.
pub fn validate(path: &str) -> Option<String> {
    JsonPath::parse(&normalize(path)).err().map(|e| format!("JSONPath syntax error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_dollar_prefix() {
        assert_eq!(normalize("items[*].price"), "$.items[*].price");
        assert_eq!(normalize("[0].x"), "$[0].x");
        assert_eq!(normalize("$.a"), "$.a");
        assert_eq!(normalize("$"), "$");
        assert_eq!(normalize("  a.b "), "$.a.b");
    }

    #[test]
    fn locate_returns_pointers_for_wildcard() {
        let doc = r#"{"items":[{"price":1},{"price":2}]}"#;
        let v: serde_json::Value = serde_json::from_str(&locate(doc, "items[*].price")).unwrap();
        let locs: Vec<&str> = v["locations"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        assert_eq!(locs, vec!["/items/0/price", "/items/1/price"]);
    }

    #[test]
    fn locate_supports_filters() {
        let doc = r#"{"items":[{"t":"a","p":1},{"t":"b","p":2}]}"#;
        let v: serde_json::Value = serde_json::from_str(&locate(doc, "items[?@.t=='b'].p")).unwrap();
        assert_eq!(v["locations"].as_array().unwrap().len(), 1);
        assert_eq!(v["locations"][0], "/items/1/p");
    }

    #[test]
    fn locate_root_is_empty_pointer() {
        let v: serde_json::Value = serde_json::from_str(&locate("{}", "$")).unwrap();
        assert_eq!(v["locations"][0], "");
    }

    #[test]
    fn locate_reports_bad_path_and_bad_doc() {
        let v: serde_json::Value = serde_json::from_str(&locate("{}", "$[")).unwrap();
        assert!(v["error"].is_string());
        let v: serde_json::Value = serde_json::from_str(&locate("not json", "$.a")).unwrap();
        assert!(v["error"].is_string());
    }

    #[test]
    fn validate_ok_and_error() {
        assert!(validate("items[*].price").is_none());
        assert!(validate("$..a[?@.b>1]").is_none());
        assert!(validate("$[").is_some());
    }
}
