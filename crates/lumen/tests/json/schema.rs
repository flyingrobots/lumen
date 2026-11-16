//! The JSON Schema subset validator (the keywords are listed in the parent module's comment).

use super::{parse, Json};

/// A schema document, checked to use only the supported keywords.
pub(crate) struct Schema {
    root: Json,
}

/// The keywords this validator applies.
const APPLIED: [&str; 18] = [
    "type",
    "const",
    "enum",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "allOf",
    "if",
    "then",
    "$ref",
    "$defs",
];

/// The annotations it reads past.
const ANNOTATIONS: [&str; 4] = ["$schema", "$id", "title", "description"];

impl Schema {
    /// Parses `text` and checks that every subschema uses only the supported keywords.
    pub(crate) fn new(text: &str) -> Result<Self, String> {
        let root = parse(text)?;
        check_keywords(&root, "#")?;
        Ok(Self { root })
    }

    /// The schema document.
    pub(crate) fn document(&self) -> &Json {
        &self.root
    }
}

fn check_keywords(schema: &Json, at: &str) -> Result<(), String> {
    let Json::Object(members) = schema else {
        return match schema {
            Json::Bool(_) => Ok(()),
            _ => Err(format!("{at}: a schema is an object or a boolean")),
        };
    };
    for (key, value) in members {
        if !APPLIED.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
            return Err(format!("{at}: unsupported keyword {key:?}"));
        }
        let here = format!("{at}/{key}");
        match (key.as_str(), value) {
            ("properties" | "$defs", Json::Object(inner)) => {
                for (name, sub) in inner {
                    check_keywords(sub, &format!("{here}/{name}"))?;
                }
            }
            ("items" | "if" | "then", sub) => check_keywords(sub, &here)?,
            ("allOf", Json::Array(subs)) => {
                for (n, sub) in subs.iter().enumerate() {
                    check_keywords(sub, &format!("{here}/{n}"))?;
                }
            }
            ("additionalProperties", Json::Bool(_)) => {}
            ("additionalProperties", _) => return Err(format!("{here}: only true or false")),
            ("$ref", Json::String(target)) if target.starts_with("#/$defs/") => {}
            ("$ref", _) => return Err(format!("{here}: only #/$defs/NAME")),
            _ => {}
        }
    }
    Ok(())
}

/// Validates `instance` against `schema`: the list of every violation, empty if it is valid.
pub(crate) fn validate(schema: &Schema, instance: &Json) -> Vec<String> {
    let mut errors = Vec::new();
    check(&schema.root, &schema.root, instance, "", &mut errors);
    errors
}

fn type_matches(name: &str, instance: &Json) -> bool {
    match (name, instance) {
        ("null", Json::Null)
        | ("boolean", Json::Bool(_))
        | ("integer", Json::Integer(_))
        | ("number", Json::Integer(_) | Json::Number(_))
        | ("string", Json::String(_))
        | ("array", Json::Array(_))
        | ("object", Json::Object(_)) => true,
        _ => false,
    }
}

fn integer(value: &Json) -> i128 {
    match value {
        Json::Integer(n) => *n,
        other => panic!("the schema needs an integer here, not {other:?}"),
    }
}

/// Every violation of `schema` by `instance`, at the JSON Pointer `at`, added to `errors`.
#[allow(clippy::too_many_lines, reason = "one arm per keyword reads best as one match")]
fn check(root: &Json, schema: &Json, instance: &Json, at: &str, errors: &mut Vec<String>) {
    let members = match schema {
        Json::Bool(true) => return,
        Json::Bool(false) => return errors.push(format!("{at}: no value is allowed")),
        Json::Object(members) => members,
        _ => unreachable!("checked by check_keywords"),
    };
    for (key, value) in members {
        match key.as_str() {
            "type" => {
                let names: Vec<&str> = match value {
                    Json::String(name) => vec![name],
                    Json::Array(items) => items
                        .iter()
                        .map(|item| match item {
                            Json::String(name) => name.as_str(),
                            _ => panic!("a type is a name"),
                        })
                        .collect(),
                    _ => panic!("type is a name or a list of names"),
                };
                if !names.iter().any(|name| type_matches(name, instance)) {
                    errors.push(format!("{at}: not of type {names:?}: {instance:?}"));
                }
            }
            "const" if instance != value => {
                errors.push(format!("{at}: {instance:?} is not {value:?}"));
            }
            "enum" => {
                let Json::Array(options) = value else { panic!("enum is a list") };
                if !options.contains(instance) {
                    errors.push(format!("{at}: {instance:?} is not one of {options:?}"));
                }
            }
            "required" => {
                let Json::Array(names) = value else { panic!("required is a list") };
                for name in names {
                    let Json::String(name) = name else { panic!("a required name") };
                    if matches!(instance, Json::Object(_)) && instance.get(name).is_none() {
                        errors.push(format!("{at}: no member {name:?}"));
                    }
                }
            }
            "properties" => {
                let Json::Object(properties) = value else { panic!("properties is an object") };
                for (name, sub) in properties {
                    if let Some(member) = instance.get(name) {
                        check(root, sub, member, &format!("{at}/{name}"), errors);
                    }
                }
            }
            "additionalProperties" => {
                let (Json::Bool(false), Json::Object(instance)) = (value, instance) else {
                    continue;
                };
                let listed = schema.get("properties");
                for (name, _) in instance {
                    if listed.and_then(|p| p.get(name)).is_none() {
                        errors.push(format!("{at}: member {name:?} is not allowed"));
                    }
                }
            }
            "items" => {
                if let Json::Array(items) = instance {
                    for (n, item) in items.iter().enumerate() {
                        check(root, value, item, &format!("{at}/{n}"), errors);
                    }
                }
            }
            "minItems" | "maxItems" => {
                if let Json::Array(items) = instance {
                    let length = i128::try_from(items.len()).expect("a length");
                    let bound = integer(value);
                    let fails = if key == "minItems" { length < bound } else { length > bound };
                    if fails {
                        errors.push(format!("{at}: {length} items breaks {key} {bound}"));
                    }
                }
            }
            "minLength" | "maxLength" => {
                if let Json::String(text) = instance {
                    let length = i128::try_from(text.chars().count()).expect("a length");
                    let bound = integer(value);
                    let fails = if key == "minLength" { length < bound } else { length > bound };
                    if fails {
                        errors.push(format!("{at}: length {length} breaks {key} {bound}"));
                    }
                }
            }
            "minimum" | "maximum" => {
                if let Json::Integer(n) = instance {
                    let bound = integer(value);
                    let fails = if key == "minimum" { *n < bound } else { *n > bound };
                    if fails {
                        errors.push(format!("{at}: {n} breaks {key} {bound}"));
                    }
                }
            }
            "allOf" => {
                let Json::Array(subs) = value else { panic!("allOf is a list") };
                for sub in subs {
                    check(root, sub, instance, at, errors);
                }
            }
            "if" => {
                let mut trial = Vec::new();
                check(root, value, instance, at, &mut trial);
                if let (true, Some(then)) = (trial.is_empty(), schema.get("then")) {
                    check(root, then, instance, at, errors);
                }
            }
            "$ref" => {
                let Json::String(target) = value else { panic!("$ref is a string") };
                let name = target.strip_prefix("#/$defs/").expect("checked");
                let sub = root
                    .get("$defs")
                    .and_then(|defs| defs.get(name))
                    .unwrap_or_else(|| panic!("no definition {name}"));
                check(root, sub, instance, at, errors);
            }
            _ => {}
        }
    }
}
