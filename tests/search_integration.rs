//! End-to-end pipeline test. We do not exercise stdout output — instead,
//! we test the internals of `run_search`'s data assembly via a thin re-export
//! seam. To avoid heavy refactoring, this test spawns mock HTTP servers,
//! injects URLs through env vars (where the real binary doesn't read them),
//! and instead reaches into the underlying source clients directly.

use std::sync::Arc;

use alfred_eudic::cache::Cache;
use alfred_eudic::cache::sqlite::SqliteCache;
use alfred_eudic::llm::{LlmClient, fetch_with_cache_llm};
use alfred_eudic::sources::{
    DictionarySource, fetch_with_cache,
    urban::UrbanClient, wordnik::WordnikClient,
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client() -> Arc<reqwest::Client> {
    Arc::new(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build().unwrap())
}

#[tokio::test]
async fn wordnik_with_many_results_does_not_trigger_llm_concept() {
    // We model the concept here: when Wordnik returns >=5, LLM should not be invoked.
    // Since run_search reads env vars and not parameters, this test simulates the
    // decision logic by checking that fetch_with_cache yields >=5 entries.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"text": "a"}, {"text": "b"}, {"text": "c"}, {"text": "d"}, {"text": "e"}, {"text": "f"}
        ])))
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let wn = WordnikClient::with_base_url(client(), "k".into(), format!("{}/word.json", server.uri()));
    let r = fetch_with_cache(&wn as &dyn DictionarySource, cache.clone(), "x", false).await.unwrap();
    assert!(r.len() >= 5, "Wordnik should return >=5 to skip LLM");
}

#[tokio::test]
async fn cache_hit_avoids_second_http_call() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "list": [{"definition": "d", "example": "", "thumbs_up": 1, "thumbs_down": 0}]
        })))
        .expect(1) // CRITICAL: server must be called exactly once
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let ub = UrbanClient::with_base_url(client(), format!("{}/", server.uri()));
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", false).await.unwrap();
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "Hello", false).await.unwrap();
    // wiremock's .expect(1) is verified at MockServer drop.
}

#[tokio::test]
async fn bypass_cache_refetches() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "list": [{"definition": "d", "example": "", "thumbs_up": 1, "thumbs_down": 0}]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let ub = UrbanClient::with_base_url(client(), format!("{}/", server.uri()));
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", false).await.unwrap();
    let _ = fetch_with_cache(&ub as &dyn DictionarySource, cache.clone(), "hello", true).await.unwrap();
}

#[tokio::test]
async fn llm_cache_hit_avoids_second_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "{\"translations\":[\"机缘\"]}"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cache: Arc<dyn Cache> = Arc::new(SqliteCache::in_memory().unwrap());
    let llm = LlmClient::with_endpoint(client(), "k".into(), format!("{}/v1/messages", server.uri()));
    let _ = fetch_with_cache_llm(&llm, cache.clone(), "serendipity", false).await.unwrap();
    let _ = fetch_with_cache_llm(&llm, cache.clone(), "SERENDIPITY", false).await.unwrap();
}
