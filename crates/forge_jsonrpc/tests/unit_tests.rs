#[cfg(test)]
mod tests {
    use forge_api::API;
    use forge_jsonrpc::test_utils::{MockAPI, create_test_server, create_test_server_with_mock};

    #[test]
    fn test_server_creation() {
        let server = create_test_server();
        // Server should be created successfully
        let _module = server.into_module();
    }

    #[test]
    fn test_server_with_custom_mock() {
        let mock = MockAPI { authenticated: true, ..Default::default() };
        let server = create_test_server_with_mock(mock);
        let _module = server.into_module();
    }

    #[tokio::test]
    async fn test_module_has_methods() {
        let server = create_test_server();
        let module = server.into_module();

        // Check that common methods are registered
        let methods = module.method_names().collect::<Vec<_>>();

        assert!(
            methods.contains(&"get_models"),
            "get_models method should be registered"
        );
        assert!(
            methods.contains(&"get_agents"),
            "get_agents method should be registered"
        );
        assert!(
            methods.contains(&"get_tools"),
            "get_tools method should be registered"
        );
        assert!(
            methods.contains(&"discover"),
            "discover method should be registered"
        );
        assert!(
            methods.contains(&"chat.stream"),
            "chat.stream method should be registered"
        );
        assert!(
            methods.contains(&"get_conversations"),
            "get_conversations method should be registered"
        );
        assert!(
            methods.contains(&"list_workspaces"),
            "list_workspaces method should be registered"
        );
    }

    #[test]
    fn test_mock_api_default() {
        let mock = MockAPI::default();
        assert!(!mock.authenticated);
        assert!(mock.models.is_empty());
        assert!(mock.agents.is_empty());
        assert!(mock.conversations.is_empty());
        assert!(mock.workspaces.is_empty());
    }

    #[tokio::test]
    async fn test_mock_api_discover() {
        let mock = MockAPI::default();
        let files = mock.discover().await.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_api_chat() {
        let mock = MockAPI::default();
        use forge_domain::{ChatRequest, ConversationId, Event};

        let request = ChatRequest {
            event: Event::empty(),
            conversation_id: ConversationId::generate(),
        };
        let stream = mock.chat(request).await.unwrap();
        // Stream should be created
        drop(stream);
    }

    #[tokio::test]
    async fn test_mock_api_conversations() {
        let mock = MockAPI::default();
        let conversations = mock.get_conversations(None).await.unwrap();
        assert!(conversations.is_empty());
    }

    #[tokio::test]
    async fn test_mock_api_shell_command() {
        let mock = MockAPI::default();
        let output = mock
            .execute_shell_command("echo test", std::path::PathBuf::from("."))
            .await
            .unwrap();
        assert!(output.stdout.contains("Executed: echo test"));
    }

    #[tokio::test]
    async fn test_mock_api_is_authenticated() {
        let mock_unauth = MockAPI::default();
        let is_auth = mock_unauth.is_authenticated().await.unwrap();
        assert!(!is_auth);

        let mock_auth = MockAPI { authenticated: true, ..Default::default() };
        let is_auth = mock_auth.is_authenticated().await.unwrap();
        assert!(is_auth);
    }

    #[tokio::test]
    async fn test_mock_api_user_info() {
        let mock_unauth = MockAPI::default();
        let user_info = mock_unauth.user_info().await.unwrap();
        assert!(user_info.is_none());

        let mock_auth = MockAPI { authenticated: true, ..Default::default() };
        let user_info = mock_auth.user_info().await.unwrap();
        assert!(user_info.is_some());
    }

    #[tokio::test]
    async fn test_mock_api_commit() {
        let mock = MockAPI::default();
        let result = mock.commit(false, None, None, None).await.unwrap();
        assert_eq!(result.message, "test commit");
        assert!(result.has_staged_files);
    }

    #[tokio::test]
    async fn test_mock_api_compact_conversation() {
        let mock = MockAPI::default();
        use forge_domain::ConversationId;

        let result = mock
            .compact_conversation(&ConversationId::generate())
            .await
            .unwrap();
        assert_eq!(result.original_tokens, 1000);
        assert_eq!(result.compacted_tokens, 500);
        assert_eq!(result.original_messages, 20);
        assert_eq!(result.compacted_messages, 10);
    }
}
