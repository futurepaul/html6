use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;

/// Component attribute value
#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    /// String literal: as="note"
    Literal(String),
    /// Expression: from={queries.feed}
    Expression(String),
}

/// Parsed component
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub tag: String,
    pub attrs: HashMap<String, AttrValue>,
    pub self_closing: bool,
}

impl Component {
    /// Parse a component opening tag
    /// Examples:
    /// - <each from={queries.feed} as="note">
    /// - <input name="message" />
    /// - <button on_click={actions.post}>
    pub fn parse(html: &str) -> Result<Self> {
        let html = html.trim();

        // Check for self-closing
        let self_closing = html.ends_with("/>");
        let content = if self_closing {
            &html[1..html.len() - 2] // Remove < and />
        } else if html.ends_with('>') {
            &html[1..html.len() - 1] // Remove < and >
        } else {
            return Err(anyhow::anyhow!("Invalid component tag: {}", html));
        };

        // Split tag name from attributes
        let parts: Vec<&str> = content.trim().splitn(2, char::is_whitespace).collect();
        let tag = parts[0].to_string();

        let attrs = if parts.len() > 1 {
            parse_attributes(parts[1])?
        } else {
            HashMap::new()
        };

        Ok(Component {
            tag,
            attrs,
            self_closing,
        })
    }

    /// Get required attribute
    pub fn get_attr(&self, name: &str) -> Result<&AttrValue> {
        self.attrs
            .get(name)
            .context(format!("Missing required attribute '{}'", name))
    }

    /// Get attribute as expression string
    pub fn get_expr(&self, name: &str) -> Result<String> {
        match self.get_attr(name)? {
            AttrValue::Expression(expr) => Ok(expr.clone()),
            AttrValue::Literal(lit) => Ok(lit.clone()),
        }
    }

    /// Get attribute as literal string
    pub fn get_literal(&self, name: &str) -> Result<String> {
        match self.get_attr(name)? {
            AttrValue::Literal(lit) => Ok(lit.clone()),
            AttrValue::Expression(expr) => {
                Err(anyhow::anyhow!("Expected literal string, got expression: {}", expr))
            }
        }
    }

    /// Get optional attribute
    pub fn get_attr_opt(&self, name: &str) -> Option<&AttrValue> {
        self.attrs.get(name)
    }
}

/// Parse component attributes
/// Supports:
/// - name="value" (literal)
/// - name={expr} (expression)
fn parse_attributes(attrs_str: &str) -> Result<HashMap<String, AttrValue>> {
    let mut attrs = HashMap::new();

    // Regex to match attribute patterns:
    // name="value" or name={expr}
    let attr_re = Regex::new(r#"(\w+)=((?:\{[^}]+\})|(?:"[^"]*"))"#).unwrap();

    for caps in attr_re.captures_iter(attrs_str) {
        let name = caps[1].to_string();
        let value_str = &caps[2];

        let value = if value_str.starts_with('{') && value_str.ends_with('}') {
            // Expression: {expr}
            let expr = value_str[1..value_str.len() - 1].to_string();
            AttrValue::Expression(expr)
        } else if value_str.starts_with('"') && value_str.ends_with('"') {
            // Literal: "value"
            let lit = value_str[1..value_str.len() - 1].to_string();
            AttrValue::Literal(lit)
        } else {
            return Err(anyhow::anyhow!("Invalid attribute value: {}", value_str));
        };

        attrs.insert(name, value);
    }

    Ok(attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_tag() {
        let comp = Component::parse("<button>").unwrap();
        assert_eq!(comp.tag, "button");
        assert_eq!(comp.attrs.len(), 0);
        assert!(!comp.self_closing);
    }

    #[test]
    fn test_parse_self_closing() {
        let comp = Component::parse("<input />").unwrap();
        assert_eq!(comp.tag, "input");
        assert!(comp.self_closing);
    }

    #[test]
    fn test_parse_literal_attribute() {
        let comp = Component::parse(r#"<input name="message">"#).unwrap();
        assert_eq!(comp.tag, "input");
        assert_eq!(comp.attrs.len(), 1);

        match comp.get_attr("name").unwrap() {
            AttrValue::Literal(val) => assert_eq!(val, "message"),
            _ => panic!("Expected literal"),
        }
    }

    #[test]
    fn test_parse_expression_attribute() {
        let comp = Component::parse("<each from={queries.feed}>").unwrap();
        assert_eq!(comp.tag, "each");

        match comp.get_attr("from").unwrap() {
            AttrValue::Expression(expr) => assert_eq!(expr, "queries.feed"),
            _ => panic!("Expected expression"),
        }
    }

    #[test]
    fn test_parse_multiple_attributes() {
        let comp = Component::parse(r#"<each from={queries.feed} as="note">"#).unwrap();
        assert_eq!(comp.tag, "each");
        assert_eq!(comp.attrs.len(), 2);

        assert_eq!(
            comp.get_expr("from").unwrap(),
            "queries.feed"
        );
        assert_eq!(comp.get_literal("as").unwrap(), "note");
    }

    #[test]
    fn test_parse_button_with_action() {
        let comp = Component::parse(r#"<button on_click={actions.post}>"#).unwrap();
        assert_eq!(comp.tag, "button");
        assert_eq!(comp.get_expr("on_click").unwrap(), "actions.post");
    }

    #[test]
    fn test_self_closing_with_attrs() {
        let comp = Component::parse(r#"<input name="note" />"#).unwrap();
        assert_eq!(comp.tag, "input");
        assert!(comp.self_closing);
        assert_eq!(comp.get_literal("name").unwrap(), "note");
    }

    #[test]
    fn test_get_missing_attr() {
        let comp = Component::parse("<button>").unwrap();
        assert!(comp.get_attr("missing").is_err());
    }

    #[test]
    fn test_get_expr_from_literal() {
        let comp = Component::parse(r#"<input name="test">"#).unwrap();
        // get_expr should work with literals too
        assert_eq!(comp.get_expr("name").unwrap(), "test");
    }

    #[test]
    fn test_get_literal_from_expr() {
        let comp = Component::parse("<each from={queries.feed}>").unwrap();
        // get_literal should fail on expression
        assert!(comp.get_literal("from").is_err());
    }
}
