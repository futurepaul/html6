use anyhow::{anyhow, Result};
use nostr_sdk::prelude::*;

use crate::parser::ast;
use crate::runtime::{JaqEvaluator, RuntimeContext};

/// Compile an AST filter definition into a nostr-sdk Filter
pub fn compile_filter(
    filter_def: &ast::Filter,
    ctx: &RuntimeContext,
    evaluator: &mut JaqEvaluator,
) -> Result<Filter> {
    let mut filter = Filter::new();

    // Add kinds
    if let Some(kinds) = &filter_def.kinds {
        // In nostr-sdk 0.43, Kind::from expects u16 not u64
        let kinds: Vec<Kind> = kinds
            .iter()
            .filter_map(|k| {
                // Convert u64 to u16, skipping invalid values
                let kind_u16 = u16::try_from(*k).ok()?;
                Some(Kind::from(kind_u16))
            })
            .collect();
        filter = filter.kinds(kinds);
    }

    // Add authors (resolve template strings)
    if let Some(authors) = &filter_def.authors {
        let pubkeys: Vec<PublicKey> = authors
            .iter()
            .filter_map(|a| resolve_pubkey(a, ctx, evaluator).ok())
            .collect();
        if !pubkeys.is_empty() {
            filter = filter.authors(pubkeys);
        }
    }

    // Add IDs
    if let Some(ids) = &filter_def.ids {
        let event_ids: Vec<EventId> = ids
            .iter()
            .filter_map(|id| EventId::from_hex(id).ok())
            .collect();
        if !event_ids.is_empty() {
            filter = filter.ids(event_ids);
        }
    }

    // Add #e tags
    if let Some(e_tags) = &filter_def.e_tags {
        let event_ids: Vec<EventId> = e_tags
            .iter()
            .filter_map(|id| EventId::from_hex(id).ok())
            .collect();
        if !event_ids.is_empty() {
            filter = filter.events(event_ids);
        }
    }

    // Add #p tags
    if let Some(p_tags) = &filter_def.p_tags {
        let pubkeys: Vec<PublicKey> = p_tags
            .iter()
            .filter_map(|p| resolve_pubkey(p, ctx, evaluator).ok())
            .collect();
        if !pubkeys.is_empty() {
            filter = filter.pubkeys(pubkeys);
        }
    }

    // Add custom tags
    for (tag_name, values) in &filter_def.custom_tags {
        if tag_name.starts_with('#') {
            let tag_kind = &tag_name[1..];
            let tag_char = tag_kind
                .chars()
                .next()
                .ok_or_else(|| anyhow!("Empty tag name"))?;
            let single_letter_tag = SingleLetterTag::from_char(tag_char)?;

            // custom_tag expects individual strings, not a Vec
            for value in values {
                filter = filter.custom_tag(single_letter_tag, value.clone());
            }
        }
    }

    // Add time bounds
    if let Some(since) = filter_def.since {
        filter = filter.since(Timestamp::from(since));
    }

    if let Some(until) = filter_def.until {
        filter = filter.until(Timestamp::from(until));
    }

    // Add limit
    if let Some(limit) = filter_def.limit {
        filter = filter.limit(limit);
    }

    Ok(filter)
}

/// Resolve a pubkey string (either hex or template expression)
fn resolve_pubkey(
    template: &str,
    ctx: &RuntimeContext,
    evaluator: &mut JaqEvaluator,
) -> Result<PublicKey> {
    // Try parsing as direct hex pubkey first
    if let Ok(pk) = PublicKey::from_hex(template) {
        return Ok(pk);
    }

    // Try parsing as bech32 npub
    if template.starts_with("npub") {
        if let Ok(pk) = PublicKey::from_bech32(template) {
            return Ok(pk);
        }
    }

    // Otherwise, treat as expression and evaluate
    let value = evaluator.eval(template, &ctx.to_json())?;

    // Extract string from result
    let hex_or_bech32 = value
        .as_str()
        .ok_or_else(|| anyhow!("Pubkey expression must evaluate to string"))?;

    // Try parsing result as hex or bech32
    PublicKey::from_hex(hex_or_bech32)
        .or_else(|_| PublicKey::from_bech32(hex_or_bech32))
        .map_err(|e| anyhow!("Invalid pubkey: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_compile_basic_filter() {
        let filter_def = ast::Filter {
            kinds: Some(vec![1]),
            limit: Some(20),
            ..Default::default()
        };

        let ctx = RuntimeContext::default();
        let mut evaluator = JaqEvaluator::new();

        let filter = compile_filter(&filter_def, &ctx, &mut evaluator).unwrap();

        // Verify the filter was created (can't easily inspect internals)
        assert!(format!("{:?}", filter).contains("kinds"));
    }

    #[test]
    fn test_compile_filter_with_hex_pubkey() {
        let test_pubkey = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";

        let filter_def = ast::Filter {
            kinds: Some(vec![1]),
            authors: Some(vec![test_pubkey.to_string()]),
            ..Default::default()
        };

        let ctx = RuntimeContext::default();
        let mut evaluator = JaqEvaluator::new();

        let filter = compile_filter(&filter_def, &ctx, &mut evaluator).unwrap();

        assert!(format!("{:?}", filter).contains("authors"));
    }

    #[test]
    fn test_compile_filter_with_expression() {
        use serde_json::json;

        let mut ctx = RuntimeContext::default();
        ctx.user = json!({
            "pubkey": "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d"
        });

        let filter_def = ast::Filter {
            kinds: Some(vec![0]),
            authors: Some(vec!["user.pubkey".to_string()]),
            ..Default::default()
        };

        let mut evaluator = JaqEvaluator::new();
        let filter = compile_filter(&filter_def, &ctx, &mut evaluator).unwrap();

        assert!(format!("{:?}", filter).contains("authors"));
    }

    #[test]
    fn test_compile_filter_with_time_bounds() {
        let filter_def = ast::Filter {
            kinds: Some(vec![1]),
            since: Some(1700000000),
            until: Some(1700001000),
            ..Default::default()
        };

        let ctx = RuntimeContext::default();
        let mut evaluator = JaqEvaluator::new();

        let filter = compile_filter(&filter_def, &ctx, &mut evaluator).unwrap();

        assert!(format!("{:?}", filter).contains("since"));
    }
}
