use html6::parser::ast::*;

fn main() {
    // Build a sample HNMD document using the AST builders
    let doc = Document::new(
        Frontmatter::new()
            .with_filter(
                "feed",
                Filter::new()
                    .kinds(vec![1])
                    .authors(vec!["user.pubkey".to_string()])
                    .limit(20),
            )
            .with_pipe(
                "feed_content",
                Pipe::new("feed", "map(.content)"),
            )
            .with_action(
                "post_note",
                Action::new(1, "{form.note}")
                    .with_tag(vec!["client".to_string(), "hnmd".to_string()]),
            ),
        vec![
            Node::heading(1, vec![Node::text("My Feed")]),
            Node::paragraph(vec![
                Node::text("Welcome! Here are the latest notes:")
            ]),
            Node::each(
                "queries.feed",
                "note",
                vec![
                    Node::paragraph(vec![
                        Node::strong(vec![Node::text("Author: ")]),
                        Node::expr("note.pubkey"),
                    ]),
                    Node::paragraph(vec![Node::expr("note.content")]),
                ],
            ),
            Node::vstack(vec![
                Node::input("note"),
                Node::button(
                    Some("actions.post_note".to_string()),
                    vec![Node::text("Post")],
                ),
            ]),
        ],
    );

    // Serialize to JSON (this is what would be published to Nostr)
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("AST as JSON:");
    println!("{}", json);

    // Deserialize back to verify roundtrip
    let parsed: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(doc, parsed);
    println!("\n✓ Roundtrip successful!");

    // Show frontmatter summary
    println!("\nDocument summary:");
    println!("- Filters: {}", doc.frontmatter.filters.len());
    println!("- Pipes: {}", doc.frontmatter.pipes.len());
    println!("- Actions: {}", doc.frontmatter.actions.len());
    println!("- Body nodes: {}", doc.body.len());
}
