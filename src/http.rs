use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::Client;

const USER_AGENT: &str = concat!("alfred-eudic/", env!("CARGO_PKG_VERSION"));

static DICT_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    Arc::new(
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(2))
            .build()
            .expect("dict reqwest client must build"),
    )
});

static LLM_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    Arc::new(
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(8))
            .build()
            .expect("llm reqwest client must build"),
    )
});

pub fn dict_client() -> Arc<Client> {
    DICT_CLIENT.clone()
}

pub fn llm_client() -> Arc<Client> {
    LLM_CLIENT.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_are_singletons() {
        let a = dict_client();
        let b = dict_client();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn dict_and_llm_are_distinct() {
        let d = dict_client();
        let l = llm_client();
        assert!(!Arc::ptr_eq(&d, &l));
    }
}
