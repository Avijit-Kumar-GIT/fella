use serde_json::Value as V;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// The generated contract uses this deliberately small JSON Schema vocabulary.
/// No external references or schema-defined executable callbacks are resolved.
pub fn check(value: &V, schema: &V, root: &V, path: &str, depth: usize) -> Result<(), String> {
    let fail = |message: &str| Err(format!("{path}: {message}"));
    if depth > 512 {
        return fail("schema nesting exceeds the contract budget");
    }
    if let Some(discriminator) = schema.pointer("/properties/type/const") {
        if value.get("type") != Some(discriminator) {
            return fail("different component type");
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(V::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: external schema references are forbidden"))?;
        let target = root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved contract schema {reference}"))?;
        return check(value, target, root, path, depth + 1);
    }
    let matches_type = |kind: &str| match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if let Some(kind) = schema.get("type") {
        let valid = kind.as_str().map(matches_type).unwrap_or_else(|| {
            kind.as_array()
                .is_some_and(|items| items.iter().filter_map(V::as_str).any(matches_type))
        });
        if !valid {
            return fail("wrong JSON type");
        }
    }
    if schema
        .get("enum")
        .and_then(V::as_array)
        .is_some_and(|items| !items.contains(value))
        || schema.get("const").is_some_and(|item| item != value)
    {
        return fail("unsupported value: outside the declared choices");
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(choices) = schema.get(keyword).and_then(V::as_array) {
            if keyword == "anyOf" {
                if let Some(choice) = choices.iter().find(|choice| {
                    choice
                        .pointer("/properties/type/const")
                        .is_some_and(|kind| value.get("type") == Some(kind))
                }) {
                    return check(value, choice, root, path, depth + 1);
                }
            }
            let outcomes: Vec<_> = choices
                .iter()
                .map(|choice| check(value, choice, root, path, depth + 1))
                .collect();
            let count = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
            if (keyword == "anyOf" && count == 0)
                || (keyword == "oneOf" && count != 1)
                || (keyword == "allOf" && count != choices.len())
            {
                let detail = outcomes
                    .iter()
                    .filter_map(|outcome| outcome.as_ref().err())
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(format!("{path}: does not match {keyword}. {detail}"));
            }
        }
    }
    if let Some(text) = value.as_str() {
        let len = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(V::as_u64)
            .is_some_and(|min| len < min)
            || schema
                .get("maxLength")
                .and_then(V::as_u64)
                .is_some_and(|max| len > max)
        {
            return fail("text length exceeds declared limits");
        }
        if let Some(pattern) = schema.get("pattern").and_then(V::as_str) {
            static PATTERNS: OnceLock<Mutex<HashMap<String, regex::Regex>>> = OnceLock::new();
            let mut patterns = PATTERNS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .map_err(|_| "regex cache unavailable".to_string())?;
            if !patterns.contains_key(pattern) {
                patterns.insert(
                    pattern.to_owned(),
                    regex::Regex::new(pattern).map_err(|error| error.to_string())?,
                );
            }
            if !patterns[pattern].is_match(text) {
                return fail("text does not match the declared pattern");
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(V::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(V::as_f64)
                .is_some_and(|max| number > max)
            || schema
                .get("exclusiveMinimum")
                .and_then(V::as_f64)
                .is_some_and(|min| number <= min)
            || schema
                .get("exclusiveMaximum")
                .and_then(V::as_f64)
                .is_some_and(|max| number >= max)
        {
            return fail("number exceeds declared limits");
        }
    }
    if let Some(items) = value.as_array() {
        if schema
            .get("minItems")
            .and_then(V::as_u64)
            .is_some_and(|min| items.len() < min as usize)
            || schema
                .get("maxItems")
                .and_then(V::as_u64)
                .is_some_and(|max| items.len() > max as usize)
        {
            return fail("array length exceeds declared limits");
        }
        if schema.get("uniqueItems") == Some(&V::Bool(true))
            && items
                .iter()
                .enumerate()
                .any(|(index, item)| items[..index].contains(item))
        {
            return fail("array values must be unique");
        }
        if let Some(item_schema) = schema.get("items") {
            let mut errors = Vec::new();
            for (index, item) in items.iter().enumerate() {
                if let Err(error) = check(
                    item,
                    item_schema,
                    root,
                    &format!("{path}[{index}]"),
                    depth + 1,
                ) {
                    errors.push(error);
                }
                if errors.len() >= 8 {
                    break;
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(missing) = schema
            .get("required")
            .and_then(V::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .filter_map(V::as_str)
                    .find(|key| !object.contains_key(*key))
            })
        {
            return Err(format!("{path}.{missing}: missing a required property"));
        }
        let props = schema.get("properties");
        let mut errors = Vec::new();
        for (key, child) in object {
            if let Some(names) = schema.get("propertyNames") {
                if let Err(error) = check(
                    &V::String(key.clone()),
                    names,
                    root,
                    &format!("{path} property name"),
                    depth + 1,
                ) {
                    errors.push(error);
                }
            }
            if let Some(child_schema) = props.and_then(|props| props.get(key)) {
                if let Err(error) = check(
                    child,
                    child_schema,
                    root,
                    &format!("{path}.{key}"),
                    depth + 1,
                ) {
                    errors.push(error);
                }
            } else if let Some(additional) = schema.get("additionalProperties") {
                if additional == &V::Bool(false) {
                    errors.push(format!("{path}.{key}: not a supported property"));
                }
                if additional.is_object() {
                    if let Err(error) =
                        check(child, additional, root, &format!("{path}.{key}"), depth + 1)
                    {
                        errors.push(error);
                    }
                }
            }
            if errors.len() >= 8 {
                break;
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repair_reports_multiple_wrong_props_without_changing_acceptance() {
        let schema =
            serde_json::json!({"type":"object","properties":{},"additionalProperties":false});
        let candidate =
            serde_json::json!({"defaultValue":"North","options":[],"stateKey":"region"});
        let errors = check(&candidate, &schema, &schema, "control.props", 0).unwrap_err();
        for field in ["defaultValue", "options", "stateKey"] {
            assert!(
                errors.contains(&format!("control.props.{field}:")),
                "{errors}"
            );
        }
    }
}
