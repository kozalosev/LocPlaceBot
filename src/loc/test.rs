use std::collections::HashMap;
use async_trait::async_trait;
use crate::loc;
use super::SearchChain;
use super::Location;
use super::LocResult;
use super::LocFinder;

#[tokio::test]
async fn test_search_chain() {
    let global_address = "123456 Global Test Land";
    let ru_address = "123456 Russia Test Land";

    let empty_finder = stub_finder(Vec::default());
    let global_finder = stub_finder(vec![location(global_address)]);
    let ru_finder = stub_finder(vec![location(ru_address)]);

    let chain = SearchChain::new(vec![empty_finder, global_finder])
        .for_lang_code("ru", vec![ru_finder]);

    for test_data in HashMap::from([("en", global_address), ("ru", ru_address)]) {
        let result = chain.find("", test_data.0, None).await;
        assert_eq!(result.len(), 1);
        let addr = result.first().unwrap()
            .address.clone()
            .expect("address must be present in the stub data!");
        assert_eq!(addr, test_data.1.to_string());
    }
}

fn stub_finder(result: Vec<Location>) -> loc::LocFinderChainWrapper {
    loc::finder("", StubLocFinder { result })
}

fn location(address: &str) -> Location {
    Location {
        address: Some(address.to_string()),
        latitude: 100.0,
        longitude: 50.0,
    }
}

struct StubLocFinder {
    result: Vec<Location>
}

#[async_trait]
impl LocFinder for StubLocFinder {
    async fn find(&self, _: &str, _: &str, _: Option<(f64, f64)>) -> LocResult {
        Ok(self.result.clone())
    }
}