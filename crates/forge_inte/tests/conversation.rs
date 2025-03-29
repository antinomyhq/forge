use std::collections::BTreeMap;

use forge_api::{Conversation, ConversationId, Workflow};
use insta::assert_json_snapshot;

trait ConversationExt {
    fn test_new() -> Conversation;
    fn subscriber_names(&self, event: &str) -> Vec<String>;
}

impl ConversationExt for Conversation {
    fn test_new() -> Conversation {
        let workflow = include_str!("../../../forge.default.yaml");
        let workflow: Workflow = serde_yaml::from_str(workflow).unwrap();
        Conversation::new(ConversationId::generate(), workflow)
    }
    fn subscriber_names(&self, event: &str) -> Vec<String> {
        self.subscribers(event)
            .iter()
            .map(|a| a.id.as_str().to_string())
            .collect()
    }
}

#[test]
fn test_using_subpath() {
    let conversation = Conversation::test_new();
    let modes = ["act".to_string(), "plan".to_string(), "help".to_string()];
    let author = ["user".to_string()];
    let task = ["task_init".to_string(), "task_update".to_string()];

    let mut specs = Vec::new();

    for mode in &modes {
        for author in &author {
            for task in &task {
                let event = format!("{}/{}/{}", mode, author, task);
                let subscribers = conversation.subscriber_names(&event);
                specs.push((event, subscribers));
            }
        }
    }

    let json = specs.into_iter().collect::<BTreeMap<String, Vec<String>>>();

    assert_json_snapshot!(json);
}
