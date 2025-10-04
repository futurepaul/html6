use jaq_interpret::{Ctx, FilterT, RcIter, Val};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JaqError {
    #[error("Failed to parse jq expression: {0}")]
    ParseError(String),

    #[error("No filter result")]
    NoResult,

    #[error("Filter execution error: {0}")]
    ExecutionError(String),
}

pub type Result<T> = std::result::Result<T, JaqError>;

pub struct JaqEvaluator {
    cache: HashMap<String, jaq_interpret::Filter>,
    defs: jaq_interpret::ParseCtx,
}

impl Clone for JaqEvaluator {
    fn clone(&self) -> Self {
        // Create a new ParseCtx since it doesn't impl Clone
        // The cache is cloneable so we can keep it
        Self {
            cache: self.cache.clone(),
            defs: jaq_interpret::ParseCtx::new(Vec::new()),
        }
    }
}

impl JaqEvaluator {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            defs: jaq_interpret::ParseCtx::new(Vec::new()),
        }
    }

    /// Evaluate a jq expression against a JSON context
    pub fn eval(&mut self, expr: &str, context: &Value) -> Result<Value> {
        // Compile jq expression (or get from cache)
        let filter = self.compile(expr)?;

        // Convert context to jaq Val
        let val = json_to_val(context);

        // Execute filter
        let inputs = RcIter::new(core::iter::empty());
        let mut results = filter.run((Ctx::new([], &inputs), val));

        // Get first result
        let result = results
            .next()
            .ok_or(JaqError::NoResult)?
            .map_err(|e| JaqError::ExecutionError(e.to_string()))?;

        // Convert back to JSON
        Ok(val_to_json(&result))
    }

    fn compile(&mut self, expr: &str) -> Result<jaq_interpret::Filter> {
        // Check cache
        if let Some(cached) = self.cache.get(expr) {
            return Ok(cached.clone());
        }

        // Parse expression
        let (filter_ast, errs) = jaq_parse::parse(expr, jaq_parse::main());

        if !errs.is_empty() {
            let err_msg = errs
                .into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(JaqError::ParseError(err_msg));
        }

        let filter_ast = filter_ast.ok_or_else(|| JaqError::ParseError("No filter parsed".into()))?;

        // Compile filter
        let filter = self.defs.compile(filter_ast);

        // Cache and return
        self.cache.insert(expr.to_string(), filter.clone());
        Ok(filter)
    }
}

impl Default for JaqEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert serde_json::Value to jaq Val
fn json_to_val(value: &Value) -> Val {
    match value {
        Value::Null => Val::Null,
        Value::Bool(b) => Val::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Val::Int(i as isize)
            } else if let Some(f) = n.as_f64() {
                Val::Float(f)
            } else {
                Val::Null
            }
        }
        Value::String(s) => Val::Str(s.clone().into()),
        Value::Array(arr) => {
            let vals: Vec<Val> = arr.iter().map(json_to_val).collect();
            Val::Arr(vals.into())
        }
        Value::Object(obj) => {
            // Build the object using iter and collect into the proper map type
            let pairs: Vec<_> = obj
                .iter()
                .map(|(k, v)| (Rc::new(k.clone()), json_to_val(v)))
                .collect();

            // Use from_iter to build the IndexMap
            Val::obj(pairs.into_iter().collect())
        }
    }
}

/// Convert jaq Val to serde_json::Value
fn val_to_json(val: &Val) -> Value {
    match val {
        Val::Null => Value::Null,
        Val::Bool(b) => Value::Bool(*b),
        Val::Int(i) => {
            // Convert to i64 for JSON
            let i64_val = i.to_string().parse::<i64>().unwrap_or(0);
            Value::Number(i64_val.into())
        }
        Val::Float(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                Value::Number(n)
            } else {
                Value::Null
            }
        }
        Val::Num(s) => {
            // Try to parse as number
            if let Ok(i) = s.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = s.parse::<f64>() {
                if let Some(n) = serde_json::Number::from_f64(f) {
                    Value::Number(n)
                } else {
                    Value::Null
                }
            } else {
                Value::String(s.to_string())
            }
        }
        Val::Str(s) => Value::String(s.to_string()),
        Val::Arr(arr) => {
            let vals: Vec<Value> = arr.iter().map(val_to_json).collect();
            Value::Array(vals)
        }
        Val::Obj(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj.iter() {
                map.insert(k.to_string(), val_to_json(v));
            }
            Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_eval_simple_path() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!({"name": "Alice"});

        let result = evaluator.eval(".name", &context).unwrap();
        assert_eq!(result, json!("Alice"));
    }

    #[test]
    fn test_eval_array_access() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!([1, 2, 3]);

        let result = evaluator.eval(".[0]", &context).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn test_eval_nested_path() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!({"user": {"name": "Bob", "age": 30}});

        let result = evaluator.eval(".user.name", &context).unwrap();
        assert_eq!(result, json!("Bob"));
    }

    #[test]
    fn test_eval_fallback() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!({});

        let result = evaluator.eval(".missing // \"default\"", &context).unwrap();
        assert_eq!(result, json!("default"));
    }

    #[test]
    fn test_cache_works() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!({"x": 1});

        // First call compiles
        evaluator.eval(".x", &context).unwrap();
        assert_eq!(evaluator.cache.len(), 1);

        // Second call uses cache
        evaluator.eval(".x", &context).unwrap();
        assert_eq!(evaluator.cache.len(), 1);
    }

    #[test]
    fn test_invalid_expression() {
        let mut evaluator = JaqEvaluator::new();
        let context = json!({});

        let result = evaluator.eval("invalid jq syntax !!!", &context);
        assert!(result.is_err());
    }
}
