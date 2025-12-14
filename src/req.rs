pub fn get_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("bar/0.1.0 (+https://github.com/Lurk/bar)")
        .build()
        .expect("Failed to build reqwest client")
}
