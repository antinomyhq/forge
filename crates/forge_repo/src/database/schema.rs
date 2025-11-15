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
    credentials (project_id) {
        project_id -> Text,
        api_key -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(conversations, credentials,);
