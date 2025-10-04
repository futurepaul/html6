use anyhow::{Context, Result};

/// Expression that can be evaluated at runtime
/// We store as strings and validate syntax, but defer actual evaluation to runtime
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Simple path: queries.feed[0].content
    Path(PathExpr),
    /// jq expression (anything more complex)
    Jq(String),
}

/// Path expression for simple member/index access
#[derive(Debug, Clone, PartialEq)]
pub struct PathExpr {
    /// Root variable name (e.g., "queries", "user", "state")
    pub root: String,
    /// Segments (field access or array indexing)
    pub segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// Field access: .field
    Field(String),
    /// Array index: [0]
    Index(usize),
}

impl Expr {
    /// Parse an expression string
    /// Simple paths like "queries.feed[0].content" become Path
    /// Everything else becomes Jq (to be evaluated by jaq at runtime)
    pub fn parse(expr: &str) -> Result<Self> {
        let trimmed = expr.trim();

        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("Empty expression"));
        }

        // Try to parse as simple path first
        if let Ok(path) = PathExpr::parse(trimmed) {
            return Ok(Expr::Path(path));
        }

        // Otherwise, treat as jq expression
        // We don't validate jq syntax here - that happens at runtime with jaq
        Ok(Expr::Jq(trimmed.to_string()))
    }

    /// Convert expression back to string
    pub fn to_string(&self) -> String {
        match self {
            Expr::Path(path) => path.to_string(),
            Expr::Jq(expr) => expr.clone(),
        }
    }

    /// Check if this is a simple path expression
    pub fn is_path(&self) -> bool {
        matches!(self, Expr::Path(_))
    }
}

impl PathExpr {
    /// Parse a path expression
    /// Examples:
    /// - "user.name" → PathExpr { root: "user", segments: [Field("name")] }
    /// - "queries.feed[0]" → PathExpr { root: "queries", segments: [Field("feed"), Index(0)] }
    /// - "queries.feed[0].content" → PathExpr { root: "queries", segments: [Field("feed"), Index(0), Field("content")] }
    pub fn parse(expr: &str) -> Result<Self> {
        let expr = expr.trim();

        // Remove leading dot if present (jq style)
        let expr = expr.strip_prefix('.').unwrap_or(expr);

        if expr.is_empty() {
            return Err(anyhow::anyhow!("Empty expression"));
        }

        let mut chars = expr.chars().peekable();
        let mut root = String::new();
        let mut segments = Vec::new();

        // Parse root identifier
        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                root.push(ch);
                chars.next();
            } else {
                break;
            }
        }

        if root.is_empty() {
            return Err(anyhow::anyhow!("Expression must start with identifier"));
        }

        // Parse segments
        while let Some(&ch) = chars.peek() {
            match ch {
                '.' => {
                    chars.next(); // consume '.'
                    let field = parse_identifier(&mut chars)?;
                    segments.push(PathSegment::Field(field));
                }
                '[' => {
                    chars.next(); // consume '['
                    let index = parse_index(&mut chars)?;
                    segments.push(PathSegment::Index(index));
                }
                _ => {
                    // Invalid character for path expression
                    return Err(anyhow::anyhow!("Invalid character in path: '{}'", ch));
                }
            }
        }

        Ok(PathExpr { root, segments })
    }

    /// Convert path back to string
    pub fn to_string(&self) -> String {
        let mut result = self.root.clone();
        for segment in &self.segments {
            match segment {
                PathSegment::Field(field) => {
                    result.push('.');
                    result.push_str(field);
                }
                PathSegment::Index(idx) => {
                    result.push('[');
                    result.push_str(&idx.to_string());
                    result.push(']');
                }
            }
        }
        result
    }
}

/// Parse an identifier (field name)
fn parse_identifier(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String> {
    let mut ident = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    if ident.is_empty() {
        return Err(anyhow::anyhow!("Expected identifier"));
    }

    Ok(ident)
}

/// Parse an array index: [123]
fn parse_index(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<usize> {
    let mut num_str = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            chars.next();
        } else if ch == ']' {
            chars.next(); // consume ']'
            break;
        } else {
            return Err(anyhow::anyhow!("Invalid character in array index: '{}'", ch));
        }
    }

    if num_str.is_empty() {
        return Err(anyhow::anyhow!("Empty array index"));
    }

    num_str
        .parse()
        .context("Failed to parse array index as number")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_path() {
        let expr = Expr::parse("user.name").unwrap();
        assert!(expr.is_path());

        match expr {
            Expr::Path(path) => {
                assert_eq!(path.root, "user");
                assert_eq!(path.segments.len(), 1);
                assert_eq!(path.segments[0], PathSegment::Field("name".to_string()));
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_parse_nested_path() {
        let expr = Expr::parse("user.profile.display_name").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.root, "user");
                assert_eq!(path.segments.len(), 2);
                assert_eq!(path.segments[0], PathSegment::Field("profile".to_string()));
                assert_eq!(
                    path.segments[1],
                    PathSegment::Field("display_name".to_string())
                );
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_parse_array_index() {
        let expr = Expr::parse("queries.feed[0]").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.root, "queries");
                assert_eq!(path.segments.len(), 2);
                assert_eq!(path.segments[0], PathSegment::Field("feed".to_string()));
                assert_eq!(path.segments[1], PathSegment::Index(0));
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_parse_complex_path() {
        let expr = Expr::parse("queries.feed[0].content").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.root, "queries");
                assert_eq!(path.segments.len(), 3);
                assert_eq!(path.segments[0], PathSegment::Field("feed".to_string()));
                assert_eq!(path.segments[1], PathSegment::Index(0));
                assert_eq!(path.segments[2], PathSegment::Field("content".to_string()));
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_parse_with_leading_dot() {
        let expr = Expr::parse(".user.name").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.root, "user");
                assert_eq!(path.segments.len(), 1);
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_parse_jq_expression() {
        // Operators make it jq
        let expr = Expr::parse("user.name // \"Anon\"").unwrap();
        match expr {
            Expr::Jq(jq) => {
                assert_eq!(jq, "user.name // \"Anon\"");
            }
            _ => panic!("Expected Jq"),
        }
    }

    #[test]
    fn test_parse_jq_filter() {
        let expr = Expr::parse("map(.content)").unwrap();
        match expr {
            Expr::Jq(jq) => {
                assert_eq!(jq, "map(.content)");
            }
            _ => panic!("Expected Jq"),
        }
    }

    #[test]
    fn test_path_to_string() {
        let path = PathExpr {
            root: "queries".to_string(),
            segments: vec![
                PathSegment::Field("feed".to_string()),
                PathSegment::Index(0),
                PathSegment::Field("content".to_string()),
            ],
        };
        assert_eq!(path.to_string(), "queries.feed[0].content");
    }

    #[test]
    fn test_expr_roundtrip() {
        let inputs = vec![
            "user.name",
            "queries.feed[0]",
            "state.items[5].title",
            "form.message",
        ];

        for input in inputs {
            let expr = Expr::parse(input).unwrap();
            assert_eq!(expr.to_string(), input);
        }
    }

    #[test]
    fn test_invalid_expressions() {
        assert!(Expr::parse("").is_err());

        // Single dot should parse as Jq
        let expr = Expr::parse(".").unwrap();
        assert!(matches!(expr, Expr::Jq(_)));

        // These should parse as Jq (not fail), since they contain operators
        let expr = Expr::parse("user..name").unwrap();
        assert!(matches!(expr, Expr::Jq(_)));
    }

    #[test]
    fn test_empty_array_index() {
        // items[] is not a valid path, should parse as Jq
        let result = Expr::parse("items[]").unwrap();
        assert!(matches!(result, Expr::Jq(_)));
    }
}
