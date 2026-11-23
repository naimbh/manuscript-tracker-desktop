//! The error type every handler returns. It reaches the renderer as
//! `{ "kind": "validation", "errors": { field: [msg, ...] } }` or `{ "kind": "error", "message": "..." }`.

use serde::ser::{Serialize, SerializeMap, Serializer};

/// Validation errors in rule order (field -> messages), like Laravel's MessageBag.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Errors(pub Vec<(String, Vec<String>)>);

impl Errors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        let field = field.into();
        let message = message.into();
        match self.0.iter_mut().find(|(f, _)| *f == field) {
            Some((_, list)) => list.push(message),
            None => self.0.push((field, vec![message])),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has(&self, field: &str) -> bool {
        self.0.iter().any(|(f, _)| f == field)
    }

    /// First message for a field.
    pub fn first(&self, field: &str) -> Option<&str> {
        self.0.iter().find(|(f, _)| f == field).and_then(|(_, m)| m.first()).map(String::as_str)
    }

    /// `Ok(())` when empty, otherwise `Err(ApiError::Validation)`.
    pub fn into_result(self) -> Result<(), ApiError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(ApiError::Validation(self))
        }
    }
}

impl Serialize for Errors {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (field, messages) in &self.0 {
            map.serialize_entry(field, messages)?;
        }
        map.end()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("The given data was invalid.")]
    Validation(Errors),
    #[error("{0}")]
    Message(String),
}

impl ApiError {
    /// A plain error shown to the user (toast).
    pub fn msg(message: impl Into<String>) -> Self {
        ApiError::Message(message.into())
    }

    /// A validation error on one field.
    pub fn field(field: impl Into<String>, message: impl Into<String>) -> Self {
        let mut e = Errors::new();
        e.add(field, message);
        ApiError::Validation(e)
    }

    pub fn not_found() -> Self {
        ApiError::Message("Not found.".into())
    }
}

impl Serialize for ApiError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(2))?;
        match self {
            ApiError::Validation(errors) => {
                map.serialize_entry("kind", "validation")?;
                map.serialize_entry("errors", errors)?;
            }
            ApiError::Message(message) => {
                map.serialize_entry("kind", "error")?;
                map.serialize_entry("message", message)?;
            }
        }
        map.end()
    }
}

macro_rules! from_error {
    ($($t:ty => $prefix:expr),* $(,)?) => {$(
        impl From<$t> for ApiError {
            fn from(e: $t) -> Self {
                ApiError::Message(format!("{}{}", $prefix, e))
            }
        }
    )*};
}

from_error! {
    rusqlite::Error => "Database error: ",
    std::io::Error => "File error: ",
    serde_json::Error => "Data error: ",
    reqwest::Error => "Network error: ",
    tauri::Error => "",
}

impl From<String> for ApiError {
    fn from(e: String) -> Self {
        ApiError::Message(e)
    }
}

impl From<&str> for ApiError {
    fn from(e: &str) -> Self {
        ApiError::Message(e.to_string())
    }
}

pub type Result<T, E = ApiError> = std::result::Result<T, E>;
