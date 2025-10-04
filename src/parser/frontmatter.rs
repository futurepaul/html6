use crate::parser::ast::{Action, Filter, Frontmatter, Pipe};
use anyhow::{Context, Result};
use serde_yaml_ng::Value;
use std::collections::HashMap;

/// Parse YAML frontmatter into Frontmatter struct
pub fn parse_frontmatter(yaml: &str) -> Result<Frontmatter> {
    let value: Value = serde_yaml_ng::from_str(yaml)
        .context("Failed to parse YAML frontmatter")?;

    let obj = value
        .as_mapping()
        .context("Frontmatter must be a YAML mapping")?;

    Ok(Frontmatter {
        filters: parse_filters(obj.get(&Value::String("filters".to_string())))?,
        pipes: parse_pipes(obj.get(&Value::String("pipes".to_string())))?,
        actions: parse_actions(obj.get(&Value::String("actions".to_string())))?,
        state: parse_state(obj.get(&Value::String("state".to_string())))?,
    })
}

/// Parse filters section
fn parse_filters(value: Option<&Value>) -> Result<HashMap<String, Filter>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };

    let mapping = value
        .as_mapping()
        .context("filters must be a mapping")?;

    let mut filters = HashMap::new();

    for (key, val) in mapping {
        let key_str = key
            .as_str()
            .context("filter key must be a string")?
            .to_string();

        let filter = parse_filter(val)?;
        filters.insert(key_str, filter);
    }

    Ok(filters)
}

/// Parse a single filter definition
fn parse_filter(value: &Value) -> Result<Filter> {
    let obj = value
        .as_mapping()
        .context("filter must be a mapping")?;

    let mut filter = Filter::new();

    // Parse kinds
    if let Some(kinds_val) = obj.get(&Value::String("kinds".to_string())) {
        let kinds = kinds_val
            .as_sequence()
            .context("kinds must be an array")?
            .iter()
            .map(|v| {
                v.as_u64()
                    .context("kind must be a number")
                    .map(|n| n as u64)
            })
            .collect::<Result<Vec<_>>>()?;
        filter.kinds = Some(kinds);
    }

    // Parse authors
    if let Some(authors_val) = obj.get(&Value::String("authors".to_string())) {
        let authors = authors_val
            .as_sequence()
            .context("authors must be an array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .context("author must be a string")
                    .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        filter.authors = Some(authors);
    }

    // Parse IDs
    if let Some(ids_val) = obj.get(&Value::String("ids".to_string())) {
        let ids = ids_val
            .as_sequence()
            .context("ids must be an array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .context("id must be a string")
                    .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        filter.ids = Some(ids);
    }

    // Parse #e tags
    if let Some(e_val) = obj.get(&Value::String("#e".to_string())) {
        let e_tags = e_val
            .as_sequence()
            .context("#e must be an array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .context("#e tag must be a string")
                    .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        filter.e_tags = Some(e_tags);
    }

    // Parse #p tags
    if let Some(p_val) = obj.get(&Value::String("#p".to_string())) {
        let p_tags = p_val
            .as_sequence()
            .context("#p must be an array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .context("#p tag must be a string")
                    .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        filter.p_tags = Some(p_tags);
    }

    // Parse since
    if let Some(since_val) = obj.get(&Value::String("since".to_string())) {
        filter.since = Some(
            since_val
                .as_u64()
                .context("since must be a number")? as u64,
        );
    }

    // Parse until
    if let Some(until_val) = obj.get(&Value::String("until".to_string())) {
        filter.until = Some(
            until_val
                .as_u64()
                .context("until must be a number")? as u64,
        );
    }

    // Parse limit
    if let Some(limit_val) = obj.get(&Value::String("limit".to_string())) {
        filter.limit = Some(
            limit_val
                .as_u64()
                .context("limit must be a number")? as usize,
        );
    }

    // TODO: Parse custom tags (#a, #t, etc.)

    Ok(filter)
}

/// Parse pipes section
fn parse_pipes(value: Option<&Value>) -> Result<HashMap<String, Pipe>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };

    let mapping = value.as_mapping().context("pipes must be a mapping")?;

    let mut pipes = HashMap::new();

    for (key, val) in mapping {
        let key_str = key
            .as_str()
            .context("pipe key must be a string")?
            .to_string();

        let pipe = parse_pipe(val)?;
        pipes.insert(key_str, pipe);
    }

    Ok(pipes)
}

/// Parse a single pipe definition
fn parse_pipe(value: &Value) -> Result<Pipe> {
    let obj = value.as_mapping().context("pipe must be a mapping")?;

    let from = obj
        .get(&Value::String("from".to_string()))
        .context("pipe must have 'from' field")?
        .as_str()
        .context("pipe 'from' must be a string")?
        .to_string();

    let jq = obj
        .get(&Value::String("jq".to_string()))
        .context("pipe must have 'jq' field")?
        .as_str()
        .context("pipe 'jq' must be a string")?
        .to_string();

    Ok(Pipe::new(from, jq))
}

/// Parse actions section
fn parse_actions(value: Option<&Value>) -> Result<HashMap<String, Action>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };

    let mapping = value
        .as_mapping()
        .context("actions must be a mapping")?;

    let mut actions = HashMap::new();

    for (key, val) in mapping {
        let key_str = key
            .as_str()
            .context("action key must be a string")?
            .to_string();

        let action = parse_action(val)?;
        actions.insert(key_str, action);
    }

    Ok(actions)
}

/// Parse a single action definition
fn parse_action(value: &Value) -> Result<Action> {
    let obj = value
        .as_mapping()
        .context("action must be a mapping")?;

    let kind = obj
        .get(&Value::String("kind".to_string()))
        .context("action must have 'kind' field")?
        .as_u64()
        .context("action 'kind' must be a number")? as u64;

    let content = obj
        .get(&Value::String("content".to_string()))
        .context("action must have 'content' field")?
        .as_str()
        .context("action 'content' must be a string")?
        .to_string();

    let mut action = Action::new(kind, content);

    // Parse tags
    if let Some(tags_val) = obj.get(&Value::String("tags".to_string())) {
        let tags = tags_val
            .as_sequence()
            .context("tags must be an array")?
            .iter()
            .map(|tag_val| {
                tag_val
                    .as_sequence()
                    .context("tag must be an array")?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .context("tag value must be a string")
                            .map(|s| s.to_string())
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;

        action.tags = tags;
    }

    Ok(action)
}

/// Parse state section
fn parse_state(value: Option<&Value>) -> Result<HashMap<String, serde_json::Value>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };

    // Convert serde_yaml::Value to serde_json::Value
    let json_str = serde_json::to_string(value)
        .context("Failed to convert YAML to JSON")?;
    let json_value: serde_json::Value = serde_json::from_str(&json_str)
        .context("Failed to parse JSON")?;

    let obj = json_value
        .as_object()
        .context("state must be an object")?;

    Ok(obj.clone().into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_frontmatter() {
        let yaml = "{}";
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.filters.len(), 0);
        assert_eq!(fm.pipes.len(), 0);
        assert_eq!(fm.actions.len(), 0);
        assert_eq!(fm.state.len(), 0);
    }

    #[test]
    fn test_parse_filter() {
        let yaml = r#"
filters:
  feed:
    kinds: [1]
    authors: ["user.pubkey"]
    limit: 20
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.filters.len(), 1);

        let feed = fm.filters.get("feed").unwrap();
        assert_eq!(feed.kinds, Some(vec![1]));
        assert_eq!(feed.authors, Some(vec!["user.pubkey".to_string()]));
        assert_eq!(feed.limit, Some(20));
    }

    #[test]
    fn test_parse_filter_with_tags() {
        let yaml = r#"
filters:
  replies:
    kinds: [1]
    '#e': ["event_id_here"]
    '#p': ["pubkey_here"]
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        let replies = fm.filters.get("replies").unwrap();
        assert_eq!(replies.e_tags, Some(vec!["event_id_here".to_string()]));
        assert_eq!(replies.p_tags, Some(vec!["pubkey_here".to_string()]));
    }

    #[test]
    fn test_parse_pipe() {
        let yaml = r#"
pipes:
  feed_content:
    from: feed
    jq: "map(.content)"
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.pipes.len(), 1);

        let pipe = fm.pipes.get("feed_content").unwrap();
        assert_eq!(pipe.from, "feed");
        assert_eq!(pipe.jq, "map(.content)");
    }

    #[test]
    fn test_parse_action() {
        let yaml = r#"
actions:
  post_note:
    kind: 1
    content: "{form.note}"
    tags:
      - ["client", "hnmd"]
      - ["t", "test"]
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.actions.len(), 1);

        let action = fm.actions.get("post_note").unwrap();
        assert_eq!(action.kind, 1);
        assert_eq!(action.content, "{form.note}");
        assert_eq!(action.tags.len(), 2);
        assert_eq!(action.tags[0], vec!["client", "hnmd"]);
        assert_eq!(action.tags[1], vec!["t", "test"]);
    }

    #[test]
    fn test_parse_state() {
        let yaml = r#"
state:
  count: 0
  selected: null
  items:
    - one
    - two
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.state.len(), 3);
        assert_eq!(fm.state.get("count").unwrap(), &serde_json::json!(0));
        assert_eq!(fm.state.get("selected").unwrap(), &serde_json::json!(null));
        assert_eq!(
            fm.state.get("items").unwrap(),
            &serde_json::json!(["one", "two"])
        );
    }

    #[test]
    fn test_parse_complete_frontmatter() {
        let yaml = r#"
filters:
  feed:
    kinds: [1]
    limit: 20
  profile:
    kinds: [0]
    authors: ["user.pubkey"]

pipes:
  feed_parsed:
    from: feed
    jq: "map({content: .content, author: .pubkey})"

actions:
  post:
    kind: 1
    content: "Hello"

state:
  active: true
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.filters.len(), 2);
        assert_eq!(fm.pipes.len(), 1);
        assert_eq!(fm.actions.len(), 1);
        assert_eq!(fm.state.len(), 1);
    }

    #[test]
    fn test_invalid_yaml() {
        let yaml = "this is not valid yaml: [[[";
        assert!(parse_frontmatter(yaml).is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        // Pipe without 'from'
        let yaml = r#"
pipes:
  bad_pipe:
    jq: ".[0]"
"#;
        assert!(parse_frontmatter(yaml).is_err());

        // Action without 'kind'
        let yaml = r#"
actions:
  bad_action:
    content: "test"
"#;
        assert!(parse_frontmatter(yaml).is_err());
    }
}
