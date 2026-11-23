//! A tiny Laravel-like validator with Laravel's default English messages.
//!
//! ```ignore
//! use crate::validation::{validate, Validator};
//!
//! // Shorthand
//! let v = validate(args, &[
//!     ("title", "required|string|max:255"),
//!     ("email", "nullable|email|max:255"),
//!     ("stage", "required|in:idea,drafting,ready"),
//!     ("roles", "nullable|array"),
//!     ("roles.*", "string|max:60"),
//! ])?;                                   // Err(ApiError::Validation) -> { kind: "validation", errors: {...} }
//! let title = v.string("title");          // "" when missing
//! let email = v.opt("email");             // None for null / blank
//!
//! // With custom messages / attribute names
//! let v = Validator::new(args)
//!     .rule("scholar_url", "nullable|url|max:255")
//!     .message("scholar_url.url", "Use your profile link.")
//!     .attribute("fileKind", "kind")
//!     .validate()?;
//! ```
//!
//! Supported rules: required, nullable, sometimes, string, max:N, min:N, email, url, date, integer, numeric,
//! boolean, in:a,b, array. Like Laravel, blank strings and missing keys skip every rule except `required`,
//! and `nullable` lets `null` through.

use crate::error::{ApiError, Errors, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Rule {
    Required,
    Nullable,
    Sometimes,
    String,
    Max(f64),
    Min(f64),
    Email,
    Url,
    Date,
    Integer,
    Numeric,
    Boolean,
    In(Vec<String>),
    Array,
}

impl Rule {
    fn parse(s: &str) -> Rule {
        let (name, arg) = s.split_once(':').unwrap_or((s, ""));
        let num = || arg.trim().parse::<f64>().unwrap_or_else(|_| panic!("rule {s} needs a number"));
        match name.trim() {
            "required" => Rule::Required,
            "nullable" => Rule::Nullable,
            "sometimes" => Rule::Sometimes,
            "string" => Rule::String,
            "max" => Rule::Max(num()),
            "min" => Rule::Min(num()),
            "email" => Rule::Email,
            "url" => Rule::Url,
            "date" => Rule::Date,
            "integer" => Rule::Integer,
            "numeric" => Rule::Numeric,
            "boolean" | "bool" => Rule::Boolean,
            "in" => Rule::In(arg.split(',').map(|x| x.trim().to_string()).collect()),
            "array" => Rule::Array,
            other => panic!("unknown validation rule: {other}"),
        }
    }

    fn key(&self) -> &'static str {
        match self {
            Rule::Required => "required",
            Rule::Nullable => "nullable",
            Rule::Sometimes => "sometimes",
            Rule::String => "string",
            Rule::Max(_) => "max",
            Rule::Min(_) => "min",
            Rule::Email => "email",
            Rule::Url => "url",
            Rule::Date => "date",
            Rule::Integer => "integer",
            Rule::Numeric => "numeric",
            Rule::Boolean => "boolean",
            Rule::In(_) => "in",
            Rule::Array => "array",
        }
    }
}

pub struct Validator<'a> {
    data: &'a Value,
    rules: Vec<(String, Vec<Rule>)>,
    messages: HashMap<String, String>,
    attributes: HashMap<String, String>,
}

/// Validate `data` against `(field, "rule|rule")` pairs.
pub fn validate(data: &Value, rules: &[(&str, &str)]) -> Result<Valid> {
    let mut v = Validator::new(data);
    for (f, r) in rules {
        v = v.rule(f, r);
    }
    v.validate()
}

impl<'a> Validator<'a> {
    pub fn new(data: &'a Value) -> Self {
        Self { data, rules: Vec::new(), messages: HashMap::new(), attributes: HashMap::new() }
    }

    /// Add rules for a field (`"title"`, `"meta.year"`, or `"roles.*"`).
    pub fn rule(mut self, field: &str, rules: &str) -> Self {
        let parsed = rules.split('|').filter(|r| !r.trim().is_empty()).map(Rule::parse).collect();
        self.rules.push((field.to_string(), parsed));
        self
    }

    /// Custom message for `"field.rule"` or `"rule"`. `:attribute`, `:max`, `:min` are replaced.
    pub fn message(mut self, key: &str, message: &str) -> Self {
        self.messages.insert(key.to_string(), message.to_string());
        self
    }

    /// Display name for a field in messages.
    pub fn attribute(mut self, field: &str, name: &str) -> Self {
        self.attributes.insert(field.to_string(), name.to_string());
        self
    }

    /// Run the rules. Returns the validated fields (present keys only) or a validation error.
    pub fn validate(self) -> Result<Valid> {
        let mut errors = Errors::new();
        let mut out = Map::new();
        for (field, rules) in &self.rules {
            for (path, value) in expand(self.data, field) {
                self.check(&path, field, value, rules, &mut errors);
                if let Some(v) = value {
                    if !path.contains('.') {
                        out.insert(path.clone(), v.clone());
                    } else if let Some(top) = path.split('.').next() {
                        if let Some(v) = self.data.get(top) {
                            out.insert(top.to_string(), v.clone());
                        }
                    }
                }
            }
        }
        errors.into_result().map(|_| Valid(out))
    }

    fn check(&self, path: &str, field: &str, value: Option<&Value>, rules: &[Rule], errors: &mut Errors) {
        let has = |r: &Rule| rules.iter().any(|x| std::mem::discriminant(x) == std::mem::discriminant(r));
        if has(&Rule::Sometimes) && value.is_none() {
            return;
        }
        let empty = match value {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.trim().is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::Object(o)) => o.is_empty(),
            _ => false,
        };
        if has(&Rule::Required) && empty {
            errors.add(path, self.msg(path, field, &Rule::Required, value));
            return;
        }
        let Some(value) = value else { return };
        if let Value::String(s) = value {
            if s.trim().is_empty() {
                return;
            }
        }
        if value.is_null() && has(&Rule::Nullable) {
            return;
        }
        let numeric_rules = has(&Rule::Numeric) || has(&Rule::Integer);
        for rule in rules {
            let ok = match rule {
                Rule::Required | Rule::Nullable | Rule::Sometimes => true,
                Rule::String => value.is_string(),
                Rule::Array => value.is_array() || value.is_object(),
                Rule::Boolean => {
                    matches!(value, Value::Bool(_))
                        || value.as_i64().is_some_and(|n| n == 0 || n == 1)
                        || value.as_str().is_some_and(|s| s == "0" || s == "1")
                }
                Rule::Integer => as_integer(value).is_some(),
                Rule::Numeric => as_number(value).is_some(),
                Rule::Email => value.as_str().is_some_and(is_email),
                Rule::Url => value.as_str().is_some_and(is_url),
                Rule::Date => value.as_str().is_some_and(is_date),
                Rule::In(list) => scalar_string(value).is_some_and(|s| list.contains(&s)),
                Rule::Max(n) => size(value, numeric_rules).is_some_and(|s| s <= *n),
                Rule::Min(n) => size(value, numeric_rules).is_some_and(|s| s >= *n),
            };
            if !ok {
                errors.add(path, self.msg(path, field, rule, Some(value)));
            }
        }
    }

    fn msg(&self, path: &str, field: &str, rule: &Rule, value: Option<&Value>) -> String {
        let key = rule.key();
        let custom = self
            .messages
            .get(&format!("{path}.{key}"))
            .or_else(|| self.messages.get(&format!("{field}.{key}")))
            .or_else(|| self.messages.get(key));
        let kind = match value {
            Some(Value::Array(_) | Value::Object(_)) => "array",
            Some(Value::Number(_)) => "numeric",
            Some(v) if as_number(v).is_some() && self.field_is_numeric(field) => "numeric",
            _ => "string",
        };
        let template = match custom {
            Some(m) => m.clone(),
            None => match (rule, kind) {
                (Rule::Required, _) => "The :attribute field is required.".into(),
                (Rule::String, _) => "The :attribute field must be a string.".into(),
                (Rule::Max(_), "numeric") => "The :attribute field must not be greater than :max.".into(),
                (Rule::Max(_), "array") => "The :attribute field must not have more than :max items.".into(),
                (Rule::Max(_), _) => "The :attribute field must not be greater than :max characters.".into(),
                (Rule::Min(_), "numeric") => "The :attribute field must be at least :min.".into(),
                (Rule::Min(_), "array") => "The :attribute field must have at least :min items.".into(),
                (Rule::Min(_), _) => "The :attribute field must be at least :min characters.".into(),
                (Rule::Email, _) => "The :attribute field must be a valid email address.".into(),
                (Rule::Url, _) => "The :attribute field must be a valid URL.".into(),
                (Rule::Date, _) => "The :attribute field must be a valid date.".into(),
                (Rule::Integer, _) => "The :attribute field must be an integer.".into(),
                (Rule::Numeric, _) => "The :attribute field must be a number.".into(),
                (Rule::Boolean, _) => "The :attribute field must be true or false.".into(),
                (Rule::In(_), _) => "The selected :attribute is invalid.".into(),
                (Rule::Array, _) => "The :attribute field must be an array.".into(),
                (Rule::Nullable | Rule::Sometimes, _) => String::new(),
            },
        };
        let attr = self.attributes.get(path).or_else(|| self.attributes.get(field)).cloned().unwrap_or_else(|| attribute_name(path));
        let mut s = template.replace(":attribute", &attr);
        if let Rule::Max(n) | Rule::Min(n) = rule {
            s = s.replace(":max", &fmt_num(*n)).replace(":min", &fmt_num(*n));
        }
        s
    }

    fn field_is_numeric(&self, field: &str) -> bool {
        self.rules.iter().any(|(f, rs)| f == field && rs.iter().any(|r| matches!(r, Rule::Numeric | Rule::Integer)))
    }
}

/// Validated input with typed accessors.
#[derive(Debug, Clone, Default)]
pub struct Valid(pub Map<String, Value>);

impl Valid {
    pub fn get(&self, k: &str) -> &Value {
        self.0.get(k).unwrap_or(&Value::Null)
    }
    pub fn has(&self, k: &str) -> bool {
        self.0.contains_key(k)
    }
    /// Trimmed string, `""` when missing or null.
    pub fn string(&self, k: &str) -> String {
        self.opt(k).unwrap_or_default()
    }
    /// Trimmed non-blank string, otherwise `None` (Laravel's `?: null`).
    pub fn opt(&self, k: &str) -> Option<String> {
        match self.get(k) {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }
    pub fn i64(&self, k: &str) -> Option<i64> {
        as_integer(self.get(k))
    }
    pub fn f64(&self, k: &str) -> Option<f64> {
        as_number(self.get(k))
    }
    /// true for true / 1 / "1" / "true" / "on".
    pub fn bool(&self, k: &str) -> bool {
        match self.get(k) {
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_i64() == Some(1),
            Value::String(s) => matches!(s.as_str(), "1" | "true" | "on" | "yes"),
            _ => false,
        }
    }
    pub fn array(&self, k: &str) -> Vec<Value> {
        self.get(k).as_array().cloned().unwrap_or_default()
    }
    pub fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

/// Resolve a field path (with `*` wildcards) to concrete paths and values.
fn expand<'v>(data: &'v Value, field: &str) -> Vec<(String, Option<&'v Value>)> {
    let mut acc: Vec<(String, Option<&'v Value>)> = vec![(String::new(), Some(data))];
    for seg in field.split('.') {
        let mut next = Vec::new();
        for (path, v) in acc {
            let join = |k: &str| {
                if path.is_empty() {
                    k.to_string()
                } else {
                    format!("{path}.{k}")
                }
            };
            if seg == "*" {
                match v {
                    Some(Value::Array(a)) => next.extend(a.iter().enumerate().map(|(i, x)| (join(&i.to_string()), Some(x)))),
                    Some(Value::Object(o)) => next.extend(o.iter().map(|(k, x)| (join(k), Some(x)))),
                    _ => {}
                }
            } else {
                let child = match v {
                    Some(Value::Object(o)) => o.get(seg),
                    Some(Value::Array(a)) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
                    _ => None,
                };
                next.push((join(seg), child));
            }
        }
        acc = next;
    }
    acc
}

/// `fileKind` / `file_kind` -> `file kind`.
pub fn attribute_name(field: &str) -> String {
    let mut out = String::with_capacity(field.len() + 4);
    for (i, c) in field.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push(' ');
            }
            out.extend(c.to_lowercase());
        } else if c == '_' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "1".into() } else { "0".into() }),
        _ => None,
    }
}

pub(crate) fn as_integer(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub(crate) fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim_start();
            if t.is_empty() || t.chars().any(|c| c.is_alphabetic() && c != 'e' && c != 'E') {
                return None;
            }
            t.parse::<f64>().ok().filter(|f| f.is_finite())
        }
        _ => None,
    }
}

fn size(v: &Value, numeric: bool) -> Option<f64> {
    match v {
        Value::Array(a) => Some(a.len() as f64),
        Value::Object(o) => Some(o.len() as f64),
        Value::Number(n) => n.as_f64(),
        Value::String(s) if numeric => as_number(v).or(Some(s.chars().count() as f64)),
        Value::String(s) => Some(s.chars().count() as f64),
        Value::Bool(_) | Value::Null => None,
    }
}

pub fn is_email(s: &str) -> bool {
    let Some((local, domain)) = s.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
        && !local.contains('@')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !local.starts_with('.')
        && !local.ends_with('.')
        && domain.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '[' || c == ']' || c == ':')
}

pub fn is_url(s: &str) -> bool {
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((scheme, rest)) = s.split_once("://") else {
        return false;
    };
    if scheme.is_empty()
        || !scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if host_port.starts_with('[') {
        host_port.split(']').next().map(|h| h.trim_start_matches('[')).unwrap_or("")
    } else {
        let (h, port) = host_port.rsplit_once(':').unwrap_or((host_port, ""));
        if !port.is_empty() && !port.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        h
    };
    !host.is_empty()
        && !host.starts_with('.')
        && !host.ends_with('-')
        && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == ':' || c == '_')
}

pub fn is_date(s: &str) -> bool {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
        || DateTime::parse_from_rfc3339(s).is_ok()
}

/// Shortcut: fail with one field error unless `ok`.
pub fn ensure(ok: bool, field: &str, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(ApiError::field(field, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn errs(data: Value, rules: &[(&str, &str)]) -> Errors {
        match validate(&data, rules) {
            Err(ApiError::Validation(e)) => e,
            Err(e) => panic!("{e}"),
            Ok(_) => Errors::new(),
        }
    }

    #[test]
    fn laravel_messages() {
        let e = errs(
            json!({ "title": "", "short_title": "x".repeat(300), "email": "nope", "scholar_url": "scholar", "stage": "zzz",
                    "decision_days": "abc", "fit": 9, "due_on": "31/12/2026", "done": "maybe", "tags": "x" }),
            &[
                ("title", "required|string|max:255"),
                ("short_title", "nullable|string|max:255"),
                ("email", "nullable|email|max:255"),
                ("scholar_url", "nullable|url|max:255"),
                ("stage", "required|in:idea,drafting"),
                ("decision_days", "nullable|integer|min:1"),
                ("fit", "integer|max:5"),
                ("due_on", "required|date"),
                ("done", "boolean"),
                ("tags", "nullable|array"),
                ("missing_field", "required"),
            ],
        );
        assert_eq!(e.first("title"), Some("The title field is required."));
        assert_eq!(e.first("short_title"), Some("The short title field must not be greater than 255 characters."));
        assert_eq!(e.first("email"), Some("The email field must be a valid email address."));
        assert_eq!(e.first("scholar_url"), Some("The scholar url field must be a valid URL."));
        assert_eq!(e.first("stage"), Some("The selected stage is invalid."));
        assert_eq!(e.first("decision_days"), Some("The decision days field must be an integer."));
        assert_eq!(e.first("fit"), Some("The fit field must not be greater than 5."));
        assert_eq!(e.first("due_on"), Some("The due on field must be a valid date."));
        assert_eq!(e.first("done"), Some("The done field must be true or false."));
        assert_eq!(e.first("tags"), Some("The tags field must be an array."));
        assert_eq!(e.first("missing_field"), Some("The missing field field is required."));
        // Errors keep rule order.
        assert_eq!(e.0[0].0, "title");
        let json = serde_json::to_value(ApiError::Validation(e)).unwrap();
        assert_eq!(json["kind"], "validation");
        assert_eq!(json["errors"]["title"][0], "The title field is required.");
    }

    #[test]
    fn passes_and_custom() {
        let data = json!({ "title": "Paper", "email": "", "url": "https://scholar.google.com/citations?user=x", "roles": ["a", "b"], "n": "12", "fileKind": "" });
        let v = validate(
            &data,
            &[
                ("title", "required|string|max:255"),
                ("email", "nullable|email"),
                ("url", "nullable|url"),
                ("roles.*", "in:a,b"),
                ("n", "integer|min:3"),
            ],
        )
        .unwrap();
        assert_eq!(v.string("title"), "Paper");
        assert_eq!(v.opt("email"), None);
        assert_eq!(v.i64("n"), Some(12));
        assert_eq!(v.array("roles").len(), 2);

        let e = match Validator::new(&data).rule("fileKind", "required").validate() {
            Err(ApiError::Validation(e)) => e,
            _ => panic!(),
        };
        assert_eq!(e.first("fileKind"), Some("The file kind field is required."));
        let e = match Validator::new(&json!({ "roles": ["a", "zz"] }))
            .rule("roles.*", "in:a,b")
            .message("roles.*.in", "Pick a CRediT role.")
            .validate()
        {
            Err(ApiError::Validation(e)) => e,
            _ => panic!(),
        };
        assert_eq!(e.first("roles.1"), Some("Pick a CRediT role."));
        assert!(validate(&json!({ "x": null }), &[("x", "nullable|string")]).is_ok());
        assert!(validate(&json!({ "x": null }), &[("x", "string")]).is_err());
        assert!(validate(&json!({ "x": "ab" }), &[("x", "min:3")]).is_err());
    }
}
