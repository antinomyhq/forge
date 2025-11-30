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
    workspaces (id) {
        id -> Integer,
        workspace_id -> BigInt,
        folder_path -> Text,
        created_at -> Timestamp,
        last_accessed_at -> Nullable<Timestamp>,
        is_active -> Bool,
    }
}

diesel::allow_tables_to_appear_in_same_query!(conversations, workspaces);
