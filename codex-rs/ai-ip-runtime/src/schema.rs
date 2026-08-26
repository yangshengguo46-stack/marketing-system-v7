use serde_json::Map;
use serde_json::Value;

const SUPPORTED_TYPES: [&str; 7] = [
    "string", "number", "boolean", "integer", "object", "array", "null",
];
const SUPPORTED_KEYWORDS: [&str; 21] = [
    "$defs",
    "$ref",
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "anyOf",
    "enum",
    "const",
    "description",
    "title",
    "pattern",
    "format",
    "multipleOf",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "minItems",
    "maxItems",
];
const UNSUPPORTED_KEYWORDS: [&str; 11] = [
    "$schema",
    "definitions",
    "oneOf",
    "allOf",
    "not",
    "dependentRequired",
    "dependentSchemas",
    "if",
    "then",
    "else",
    "patternProperties",
];

#[derive(Debug, thiserror::Error)]
pub enum StrictSchemaError {
    #[error("failed to serialize generated schema: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid Responses strict schema at {path}: {message}")]
    Invalid { path: String, message: String },
}

pub fn content_package_schema() -> Result<Value, StrictSchemaError> {
    let generated = schemars::schema_for!(codex_ai_ip_domain::ContentPackage);
    let mut schema = serde_json::to_value(generated)?;
    normalize_schema(&mut schema)?;
    validate_responses_strict_subset(&schema)?;
    Ok(schema)
}

pub fn validate_responses_strict_subset(schema: &Value) -> Result<(), StrictSchemaError> {
    let root = schema
        .as_object()
        .ok_or_else(|| invalid("$", "root must be an object schema without anyOf"))?;
    if root.contains_key("anyOf") {
        return Err(invalid("$", "root must be an object schema without anyOf"));
    }

    let definitions = match root.get("$defs") {
        Some(Value::Object(definitions)) => Some(definitions),
        Some(_) => return Err(invalid("$", "$defs must be an object")),
        None => None,
    };
    validate_schema_node(schema, definitions, "$")?;
    if matches!(root.get("type"), Some(Value::String(kind)) if kind == "object") {
        Ok(())
    } else {
        Err(invalid("$", "root must be an object schema without anyOf"))
    }
}

pub(crate) fn normalize_schema(schema: &mut Value) -> Result<(), StrictSchemaError> {
    let root = schema
        .as_object_mut()
        .ok_or_else(|| invalid("$", "schema root must be an object"))?;
    root.remove("$schema");
    if root.contains_key("$defs") {
        return Err(invalid("$", "root must not already contain $defs"));
    }

    let mut definitions = root.remove("definitions");
    if let Some(Value::Object(definitions_map)) = definitions.as_mut() {
        for definition in definitions_map.values_mut() {
            normalize_node(definition);
        }
    }
    normalize_node(schema);
    if let Some(definitions) = definitions {
        let root = schema
            .as_object_mut()
            .ok_or_else(|| invalid("$", "schema root must be an object"))?;
        root.insert("$defs".to_string(), definitions);
    }
    Ok(())
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> StrictSchemaError {
    StrictSchemaError::Invalid {
        path: path.into(),
        message: message.into(),
    }
}

fn normalize_node(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    if let Some(Value::String(reference)) = object.get_mut("$ref")
        && let Some(token) = reference.strip_prefix("#/definitions/")
    {
        *reference = format!("#/$defs/{token}");
    }

    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        let required = properties.keys().cloned().collect::<Vec<_>>();
        object.insert("additionalProperties".to_string(), Value::Bool(false));
        object.insert("required".to_string(), Value::from(required));
    } else if object.get("type") == Some(&Value::String("object".to_string())) {
        object.insert("additionalProperties".to_string(), Value::Bool(false));
        object.insert("required".to_string(), Value::Array(Vec::new()));
    }

    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            normalize_node(property);
        }
    }
    if let Some(definitions) = object.get_mut("$defs").and_then(Value::as_object_mut) {
        for definition in definitions.values_mut() {
            normalize_node(definition);
        }
    }
    if let Some(items) = object.get_mut("items") {
        normalize_node(items);
    }
    for composition in ["anyOf", "oneOf", "allOf"] {
        if let Some(variants) = object.get_mut(composition).and_then(Value::as_array_mut) {
            for variant in variants {
                normalize_node(variant);
            }
        }
    }
}

fn validate_schema_node(
    schema: &Value,
    definitions: Option<&Map<String, Value>>,
    path: &str,
) -> Result<(), StrictSchemaError> {
    let object = schema
        .as_object()
        .ok_or_else(|| invalid(path, "schema must be an object"))?;

    for keyword in UNSUPPORTED_KEYWORDS {
        if object.contains_key(keyword) {
            return Err(invalid(path, format!("unsupported keyword {keyword}")));
        }
    }
    for keyword in object.keys() {
        if !SUPPORTED_KEYWORDS.contains(&keyword.as_str()) {
            return Err(invalid(path, format!("unsupported keyword {keyword}")));
        }
    }

    if let Some(schema_type) = object.get("type") {
        validate_type(schema_type, path)?;
    }
    if let Some(reference) = object.get("$ref") {
        validate_reference(reference, definitions, path)?;
    }
    if object
        .get("required")
        .is_some_and(|required| !required.is_array())
    {
        return Err(invalid(
            path,
            "required must contain every property exactly once",
        ));
    }

    let is_object = object.contains_key("properties") || type_includes_object(object.get("type"));
    if is_object {
        let properties = object
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(path, "properties must be an object"))?;
        if object.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(invalid(
                path,
                "object must set additionalProperties to false",
            ));
        }
        validate_required(object.get("required"), properties, path)?;
    } else if let Some(properties) = object.get("properties")
        && !properties.is_object()
    {
        return Err(invalid(path, "properties must be an object"));
    }

    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| invalid(path, "properties must be an object"))?;
        for (name, property) in properties {
            validate_schema_node(property, definitions, &format!("{path}.properties.{name}"))?;
        }
    }
    if let Some(definitions_value) = object.get("$defs") {
        let definitions_map = definitions_value
            .as_object()
            .ok_or_else(|| invalid(path, "$defs must be an object"))?;
        for (name, definition) in definitions_map {
            validate_schema_node(definition, definitions, &format!("{path}.$defs.{name}"))?;
        }
    }
    if let Some(items) = object.get("items") {
        if !items.is_object() {
            return Err(invalid(path, "items must be an object schema"));
        }
        validate_schema_node(items, definitions, &format!("{path}.items"))?;
    }
    if let Some(any_of) = object.get("anyOf") {
        let variants = any_of
            .as_array()
            .filter(|variants| !variants.is_empty())
            .ok_or_else(|| invalid(path, "anyOf must be a nonempty array of object schemas"))?;
        for (index, variant) in variants.iter().enumerate() {
            if !variant.is_object() {
                return Err(invalid(
                    path,
                    "anyOf must be a nonempty array of object schemas",
                ));
            }
            validate_schema_node(variant, definitions, &format!("{path}.anyOf[{index}]"))?;
        }
    }
    Ok(())
}

fn validate_type(schema_type: &Value, path: &str) -> Result<(), StrictSchemaError> {
    let valid = match schema_type {
        Value::String(name) => SUPPORTED_TYPES.contains(&name.as_str()),
        Value::Array(names) if !names.is_empty() => {
            let mut seen = std::collections::HashSet::new();
            names.iter().all(|name| {
                name.as_str()
                    .is_some_and(|name| SUPPORTED_TYPES.contains(&name) && seen.insert(name))
            })
        }
        Value::Array(_) | Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => {
            false
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(
            path,
            "type must contain unique supported primitive names",
        ))
    }
}

fn type_includes_object(schema_type: Option<&Value>) -> bool {
    match schema_type {
        Some(Value::String(name)) => name == "object",
        Some(Value::Array(names)) => names.iter().any(|name| name == "object"),
        Some(Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_)) | None => false,
    }
}

fn validate_required(
    required: Option<&Value>,
    properties: &Map<String, Value>,
    path: &str,
) -> Result<(), StrictSchemaError> {
    let Some(required) = required.and_then(Value::as_array) else {
        return Err(invalid(
            path,
            "required must contain every property exactly once",
        ));
    };
    let mut seen = std::collections::HashSet::new();
    let exact = required.len() == properties.len()
        && required.iter().all(|name| {
            name.as_str()
                .is_some_and(|name| properties.contains_key(name) && seen.insert(name))
        });
    if exact {
        Ok(())
    } else {
        Err(invalid(
            path,
            "required must contain every property exactly once",
        ))
    }
}

fn validate_reference(
    reference: &Value,
    definitions: Option<&Map<String, Value>>,
    path: &str,
) -> Result<(), StrictSchemaError> {
    let reference = reference
        .as_str()
        .ok_or_else(|| invalid(path, "local ref must use #/$defs/"))?;
    if reference == "#" {
        return Ok(());
    }
    if reference.starts_with("#/definitions/") || !reference.starts_with("#/$defs/") {
        return Err(invalid(path, "local ref must use #/$defs/"));
    }

    let token = &reference["#/$defs/".len()..];
    if token.contains('/') {
        return Err(invalid(path, "local ref must use #/$defs/"));
    }
    let token =
        decode_pointer_token(token).ok_or_else(|| invalid(path, "invalid JSON Pointer escape"))?;
    if definitions.is_some_and(|definitions| definitions.contains_key(&token)) {
        Ok(())
    } else {
        Err(invalid(path, "dangling local ref"))
    }
}

fn decode_pointer_token(token: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut chars = token.chars();
    while let Some(character) = chars.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        match chars.next()? {
            '0' => decoded.push('~'),
            '1' => decoded.push('/'),
            _ => return None,
        }
    }
    Some(decoded)
}
