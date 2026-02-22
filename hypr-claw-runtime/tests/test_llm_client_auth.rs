#[cfg(test)]
mod llm_client_tests {
    use hypr_claw_runtime::LLMClient;

    #[test]
    fn test_llm_client_with_api_key() {
        let client = LLMClient::with_api_key(
            "https://integrate.api.nvidia.com/v1".to_string(),
            1,
            "test-api-key".to_string(),
        );
        
        // Client should be created successfully
        // API key is stored internally (not exposed)
        drop(client);
    }

    #[test]
    fn test_llm_client_without_api_key() {
        let client = LLMClient::new(
            "http://localhost:8080".to_string(),
            1,
        );
        
        // Client should be created successfully
        drop(client);
    }
}
