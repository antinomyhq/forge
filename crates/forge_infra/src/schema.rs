// @generated automatically by Diesel CLI.

diesel::table! {
    conversations (conversation_id) {
        conversation_id -> Text,
        workspace_id -> Text,
        context -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}