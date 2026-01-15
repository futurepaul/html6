use crate::runtime::jaq::{JaqEvaluator, Result as JaqResult};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Runtime context available to expressions during rendering
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub user: Value,
    pub queries: Value,
    pub state: Value,
    pub form: HashMap<String, String>,
    pub locals: HashMap<String, Value>,  // For scoped variables like "note" in <each>
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self {
            user: json!({}),
            queries: json!({}),
            state: json!({}),
            form: HashMap::new(),
            locals: HashMap::new(),
        }
    }

    /// Create a runtime context with initial state
    pub fn with_state(state: HashMap<String, Value>) -> Self {
        Self {
            user: json!({}),
            queries: json!({}),
            state: json!(state),
            form: HashMap::new(),
            locals: HashMap::new(),
        }
    }

    /// Convert context to a JSON object for jaq evaluation
    pub fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("user".to_string(), self.user.clone());
        obj.insert("queries".to_string(), self.queries.clone());
        obj.insert("state".to_string(), self.state.clone());
        obj.insert("form".to_string(), json!(self.form));

        // Add locals at the top level so they can be accessed directly (e.g., "note" not "locals.note")
        for (key, value) in &self.locals {
            obj.insert(key.clone(), value.clone());
        }

        Value::Object(obj)
    }

    /// Evaluate a jq expression against this context
    pub fn eval(&self, expr: &str, evaluator: &mut JaqEvaluator) -> JaqResult<Value> {
        // Prepend `.` if not present (for convenience)
        let jq_expr = if expr.starts_with('.') {
            expr.to_string()
        } else {
            format!(".{}", expr)
        };

        evaluator.eval(&jq_expr, &self.to_json())
    }

    /// Add a local binding to the context (for use in scoped contexts like <each>)
    pub fn with_local(&self, name: &str, value: Value) -> Self {
        let mut new_ctx = self.clone();
        new_ctx.locals.insert(name.to_string(), value);
        new_ctx
    }

    /// Update a form field value
    pub fn set_form_field(&mut self, name: &str, value: String) {
        self.form.insert(name.to_string(), value);
    }

    /// Get a form field value
    pub fn get_form_field(&self, name: &str) -> Option<&String> {
        self.form.get(name)
    }
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new_context() {
        let ctx = RuntimeContext::new();
        assert_eq!(ctx.user, json!({}));
        assert_eq!(ctx.queries, json!({}));
        assert_eq!(ctx.state, json!({}));
        assert!(ctx.form.is_empty());
    }

    #[test]
    fn test_with_state() {
        let mut state = HashMap::new();
        state.insert("count".to_string(), json!(42));
        state.insert("name".to_string(), json!("Alice"));

        let ctx = RuntimeContext::with_state(state);

        let json_ctx = ctx.to_json();
        assert_eq!(json_ctx["state"]["count"], json!(42));
        assert_eq!(json_ctx["state"]["name"], json!("Alice"));
    }

    #[test]
    fn test_eval_simple() {
        let mut state = HashMap::new();
        state.insert("message".to_string(), json!("Hello, World!"));

        let ctx = RuntimeContext::with_state(state);
        let mut evaluator = JaqEvaluator::new();

        let result = ctx.eval("state.message", &mut evaluator).unwrap();
        assert_eq!(result, json!("Hello, World!"));
    }

    #[test]
    fn test_eval_with_dot_prefix() {
        let mut state = HashMap::new();
        state.insert("x".to_string(), json!(123));

        let ctx = RuntimeContext::with_state(state);
        let mut evaluator = JaqEvaluator::new();

        // Should work with or without leading dot
        let result1 = ctx.eval(".state.x", &mut evaluator).unwrap();
        let result2 = ctx.eval("state.x", &mut evaluator).unwrap();

        assert_eq!(result1, json!(123));
        assert_eq!(result2, json!(123));
    }

    #[test]
    fn test_form_fields() {
        let mut ctx = RuntimeContext::new();

        ctx.set_form_field("username", "alice".to_string());
        ctx.set_form_field("email", "alice@example.com".to_string());

        assert_eq!(ctx.get_form_field("username"), Some(&"alice".to_string()));
        assert_eq!(ctx.get_form_field("email"), Some(&"alice@example.com".to_string()));
        assert_eq!(ctx.get_form_field("missing"), None);
    }

    #[test]
    fn test_eval_fallback() {
        let ctx = RuntimeContext::new();
        let mut evaluator = JaqEvaluator::new();

        let result = ctx.eval("state.missing // \"default\"", &mut evaluator).unwrap();
        assert_eq!(result, json!("default"));
    }
}
