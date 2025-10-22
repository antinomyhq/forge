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
    provider_credentials (id) {
        id -> Nullable<Integer>,
        provider_id -> Text,
        auth_type -> Text,
        api_key -> Nullable<Text>,
        refresh_token -> Nullable<Text>,
        access_token -> Nullable<Text>,
        token_expires_at -> Nullable<Timestamp>,
        url_params -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(conversations, provider_credentials,);
