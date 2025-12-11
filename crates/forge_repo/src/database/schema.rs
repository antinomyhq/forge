// @generated automatically by Diesel CLI.

diesel::table! {
    conversations (conversation_id) {
        conversation_id -> Text,
        title -> Nullable<Text>,
        workspace_id -> BigInt,
        context -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
        metrics -> Nullable<Text>,
    }
}

diesel::table! {
    workspace (remote_workspace_id) {
        remote_workspace_id -> Text,
        user_id -> Text,
        path -> Text,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    workspace_sync_status (path) {
        path -> Text,
        status -> Text,
        last_synced_at -> Timestamp,
        error_message -> Nullable<Text>,
        process_id -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(conversations, workspace, workspace_sync_status,);
