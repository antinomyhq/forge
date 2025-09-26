use forge_domain::Transformer;

use crate::dto::openai::{ProviderPreferences, Request};

/// A transformer that sets provider preferences for specific models
pub struct SetProviderPreferences {
    provider_preferences: ProviderPreferences,
}

impl SetProviderPreferences {
    /// Creates a new SetProviderPreferences transformer
    ///
    /// # Arguments
    /// * `order` - The provider order preference
    /// * `allow_fallbacks` - Whether to allow fallbacks
    ///
    /// # Examples
    /// ```rust,ignore
    /// let transformer = SetProviderPreferences::new(
    ///     vec!["moonshotai".to_string(), "groq".to_string()],
    ///     true
    /// );
    /// ```
    pub fn new(order: Vec<String>, allow_fallbacks: bool) -> Self {
        Self {
            provider_preferences: ProviderPreferences { order, allow_fallbacks },
        }
    }
}

impl Transformer for SetProviderPreferences {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.provider = Some(self.provider_preferences.clone());
        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::ModelId;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_set_provider_preferences() {
        // Fixture
        let mut transformer =
            SetProviderPreferences::new(vec!["moonshotai".to_string(), "groq".to_string()], true);
        let request = Request::default().model(ModelId::new("kimi-k2"));

        // Execute
        let actual = transformer.transform(request);

        // Expected: provider preferences should be set
        let expected_preferences = Some(ProviderPreferences {
            order: vec!["moonshotai".to_string(), "groq".to_string()],
            allow_fallbacks: true,
        });
        assert_eq!(actual.provider, expected_preferences);
    }

    #[test]
    fn test_set_provider_preferences_overwrites_existing() {
        // Fixture
        let mut transformer =
            SetProviderPreferences::new(vec!["moonshotai".to_string(), "groq".to_string()], true);
        let existing_preferences =
            ProviderPreferences { order: vec!["openai".to_string()], allow_fallbacks: false };
        let request = Request::default()
            .model(ModelId::new("kimi-k2"))
            .provider(existing_preferences);

        // Execute
        let actual = transformer.transform(request);

        // Expected: provider preferences should be overwritten
        let expected_preferences = Some(ProviderPreferences {
            order: vec!["moonshotai".to_string(), "groq".to_string()],
            allow_fallbacks: true,
        });
        assert_eq!(actual.provider, expected_preferences);
    }

    #[test]
    fn test_set_provider_preferences_empty_order() {
        // Fixture
        let mut transformer = SetProviderPreferences::new(vec![], false);
        let request = Request::default().model(ModelId::new("kimi-k2"));

        // Execute
        let actual = transformer.transform(request);

        // Expected: provider preferences should be set with empty order
        let expected_preferences =
            Some(ProviderPreferences { order: vec![], allow_fallbacks: false });
        assert_eq!(actual.provider, expected_preferences);
    }
}
