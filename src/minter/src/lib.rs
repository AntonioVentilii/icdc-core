use ic_cdk_macros::{query, update};

#[query]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
