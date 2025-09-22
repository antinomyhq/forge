// @generated automatically by Diesel CLI.

diesel::table! {
    conversation_stats (conversation_id) {
        conversation_id -> Text,
        workspace_id -> BigInt,
        title -> Nullable<Text>,
        message_count -> Integer,
        total_tokens -> BigInt,
        prompt_tokens -> BigInt,
        completion_tokens -> BigInt,
        cached_tokens -> BigInt,
        cost -> Nullable<Double>,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    conversations (conversation_id) {
        conversation_id -> Text,
        title -> Nullable<Text>,
        workspace_id -> BigInt,
        context -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(conversation_stats -> conversations (conversation_id));

diesel::allow_tables_to_appear_in_same_query!(
    conversation_stats,
    conversations,
);