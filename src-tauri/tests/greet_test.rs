#[test]
fn test_greeting_integration() {
    let result = yf_download_lib::format_greeting("Tauri");
    assert_eq!(result, "Hello, Tauri! You've been greeted from Rust!");
}
