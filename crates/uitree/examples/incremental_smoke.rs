//! Read-only desktop probe. Optional argument scopes descendant capture to a title.
use std::time::{Duration, Instant};
fn main() {
    let service = uitree::TreeService::new();
    let start = Instant::now();
    match service.membership(start + Duration::from_secs(10)) {
        Ok(tree) => println!(
            "membership_ms={} windows={} revision={}",
            start.elapsed().as_millis(),
            tree.children(0).len(),
            tree.revision()
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    if let Some(title) = std::env::args().nth(1) {
        let start = Instant::now();
        match service.ensure(Some(&title), start + Duration::from_secs(10)) {
            Ok(tree) => println!(
                "query_ms={} elements={} revision={}",
                start.elapsed().as_millis(),
                tree.get_elements().len(),
                tree.revision()
            ),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    println!("{}", service.status());
}
