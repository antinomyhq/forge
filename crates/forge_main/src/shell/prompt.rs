//! Shared prompt data fetching for shell integrations.
//!
//! Provides a shell-agnostic [`ShellPromptData`] struct and an async
//! [`fetch_prompt_data`] function that collects prompt information from the API
//! and environment. Each shell module (zsh, powershell, etc.) consumes this
//! data and formats it with shell-specific escape sequences.

use std::str::FromStr;

use forge_api::{API, AgentId, Conversation, ConversationId, ModelId};
use forge_domain::TokenCount;
use futures::future;

/// Shell-agnostic prompt data, collected once and passed to any shell
/// formatter.
pub struct ShellPromptData {
    pub agent: Option<AgentId>,
    pub model: Option<ModelId>,
    pub token_count: Option<TokenCount>,
    pub cost: Option<f64>,
    pub use_nerd_font: bool,
    pub currency_symbol: String,
    pub conversion_ratio: f64,
}

/// Fetches prompt data from the API and environment variables.
///
/// This extracts the common logic shared by all shell rprompt handlers:
/// reading env vars, fetching model/conversation data in parallel, and
/// computing cost across related conversations.
pub async fn fetch_prompt_data(api: &(dyn API + Send + Sync)) -> ShellPromptData {
    let cid = std::env::var("_FORGE_CONVERSATION_ID")
        .ok()
        .filter(|text| !text.trim().is_empty())
        .and_then(|str| ConversationId::from_str(str.as_str()).ok());

    // Make IO calls in parallel
    let (model_id, conversation) = tokio::join!(api.get_default_model(), async {
        if let Some(cid) = cid {
            api.conversation(&cid).await.ok().flatten()
        } else {
            None
        }
    });

    // Calculate total cost including related conversations
    let cost = if let Some(ref conv) = conversation {
        let related = fetch_related_conversations(api, conv).await;
        let all: Vec<_> = std::iter::once(conv)
            .chain(related.iter())
            .cloned()
            .collect();
        Conversation::total_cost(&all)
    } else {
        None
    };

    let agent = std::env::var("_FORGE_ACTIVE_AGENT")
        .ok()
        .filter(|text| !text.trim().is_empty())
        .map(AgentId::new);

    let use_nerd_font = std::env::var("NERD_FONT")
        .or_else(|_| std::env::var("USE_NERD_FONT"))
        .map(|val| val == "1")
        .unwrap_or(true);

    let currency_symbol =
        std::env::var("FORGE_CURRENCY_SYMBOL").unwrap_or_else(|_| "$".to_string());

    let conversion_ratio = std::env::var("FORGE_CURRENCY_CONVERSION_RATE")
        .ok()
        .and_then(|val| val.parse::<f64>().ok())
        .unwrap_or(1.0);

    let token_count = conversation.and_then(|c| c.token_count());

    ShellPromptData {
        agent,
        model: model_id,
        token_count,
        cost,
        use_nerd_font,
        currency_symbol,
        conversion_ratio,
    }
}

/// Fetches related conversations for a given conversation in parallel.
async fn fetch_related_conversations(
    api: &(dyn API + Send + Sync),
    conversation: &Conversation,
) -> Vec<Conversation> {
    let related_ids = conversation.related_conversation_ids();

    let related_futures: Vec<_> = related_ids
        .iter()
        .map(|id| {
            let id = *id;
            async move { api.conversation(&id).await }
        })
        .collect();

    future::join_all(related_futures)
        .await
        .into_iter()
        .filter_map(|result| result.ok().flatten())
        .collect()
}
