use html6::{loader, runtime::execute_all_pipes};

#[tokio::main]
async fn main() {
    // Load the document
    let (doc, _registry) = loader::load_hnmd("apps/feed.hnmd").expect("Failed to load");

    println!("\n📋 Pipes defined: {}", doc.frontmatter.pipes.len());
    for (id, pipe) in &doc.frontmatter.pipes {
        println!("  - {}: {}", id, pipe.jq);
    }

    // Create mock query runtime and wait for data
    let mut query_runtime = html6::runtime::QueryRuntime::new().await.expect("Failed to create runtime");
    query_runtime.add_relay("wss://relay.damus.io").await.expect("Failed to add relay");

    // Subscribe to feed
    let runtime_ctx = html6::runtime::RuntimeContext::new();
    if let Some(feed_filter) = doc.frontmatter.filters.get("feed") {
        println!("\n📡 Subscribing to feed...");
        query_runtime.subscribe_ast_filter("feed", feed_filter, &runtime_ctx).await.expect("Failed to subscribe");
    }

    // Wait for events and profiles to arrive
    println!("⏳ Waiting for events and profiles...");
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // Get queries JSON
    let queries_json = query_runtime.to_json().await;
    println!("\n📊 Raw Queries:");
    if let Some(obj) = queries_json.as_object() {
        for (key, value) in obj {
            if let Some(arr) = value.as_array() {
                println!("  {}: {} events", key, arr.len());
            }
        }
    }

    // Execute pipes
    println!("\n🔧 Executing pipes...");
    match execute_all_pipes(&doc.frontmatter.pipes, &queries_json) {
        Ok(enriched) => {
            println!("✅ Pipes executed successfully!");

            if let Some(obj) = enriched.as_object() {
                for (key, value) in obj {
                    if let Some(arr) = value.as_array() {
                        println!("\n📦 {}: {} items", key, arr.len());

                        // Show first 3 items as samples
                        for (i, item) in arr.iter().take(3).enumerate() {
                            println!("\n   Item {}:", i);
                            println!("{}", serde_json::to_string_pretty(item).unwrap());
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Pipe execution failed: {}", e);
            eprintln!("   Full error: {:?}", e);
        }
    }
}
