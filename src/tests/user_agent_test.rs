//! Tests for User-Agent customization

#[cfg(test)]
mod tests {
    use crate::client::ClientOption;

    #[test]
    fn test_client_option_user_agent() {
        // Test that the user_agent field can be set and retrieved
        let user_agent = "RuSocks/1.0 (Test Client)";

        let options = ClientOption::default().with_user_agent(user_agent.to_string());

        assert_eq!(options.user_agent, Some(user_agent.to_string()));
    }

    #[test]
    fn test_client_option_default() {
        // Test that the default user_agent is None
        let options = ClientOption::default();

        assert_eq!(options.user_agent, None);
    }
}
