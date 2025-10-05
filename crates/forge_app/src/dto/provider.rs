use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};
use url::Url;

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
    PartialOrd,
    Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Forge,
    #[serde(rename = "openai")]
    OpenAI,
    OpenRouter,
    Requesty,
    Zai,
    ZaiCoding,
    Cerebras,
    Xai,
    Anthropic,
    VertexAi,
    BigModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderResponse {
    OpenAI,
    Anthropic,
}

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Provider {
    pub id: ProviderId,
    pub response: ProviderResponse,
    pub url: Url,
    pub key: Option<String>,
}

impl Provider {
    pub fn forge(key: &str) -> Provider {
        Provider {
            id: ProviderId::Forge,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://antinomy.ai/api/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating a ZAI provider
    pub fn zai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Zai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub fn zai_coding(key: &str) -> Provider {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub fn openai(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating an XAI provider
    pub fn xai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating a Requesty provider
    pub fn requesty(key: &str) -> Provider {
        Provider {
            id: ProviderId::Requesty,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.requesty.ai/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating an OpenRouter provider
    pub fn open_router(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenRouter,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://openrouter.ai/api/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    /// Test helper for creating an Anthropic provider
    pub fn anthropic(key: &str) -> Provider {
        Provider {
            id: ProviderId::Anthropic,
            response: ProviderResponse::Anthropic,
            url: Url::parse("https://api.anthropic.com/v1/").unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn vertex_ai(key: &str, project_id: &str, location: &str) -> anyhow::Result<Provider> {
        let url = if location == "global" {
            format!(
                "https://aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/",
                project_id, location
            )
        } else {
            format!(
                "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/",
                location, project_id, location
            )
        };
        Ok(Provider {
            id: ProviderId::VertexAi,
            response: ProviderResponse::OpenAI,
            url: Url::parse(&url)?,
            key: Some(key.into()),
        })
    }
}

impl Provider {
    /// Converts the provider to it's base URL
    pub fn to_base_url(&self) -> Url {
        self.url.clone()
    }

    pub fn model_url(&self) -> Url {
        match &self.response {
            ProviderResponse::OpenAI => {
                if self.id == ProviderId::ZaiCoding {
                    let base_url = Url::parse("https://api.z.ai/api/paas/v4/").unwrap();
                    base_url.join("models").unwrap()
                } else {
                    self.url.join("models").unwrap()
                }
            }
            ProviderResponse::Anthropic => self.url.join("models").unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_xai() {
        let fixture = "test_key";
        let actual = Provider::xai(fixture);
        let expected = Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::from_str("https://api.x.ai/v1/").unwrap(),
            key: Some(fixture.to_string()),
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_xai_with_direct_comparison() {
        let fixture_xai = Provider::xai("key");
        assert_eq!(fixture_xai.id, ProviderId::Xai);

        let fixture_other = Provider::openai("key");
        assert_ne!(fixture_other.id, ProviderId::Xai);
    }

    #[test]
    fn test_zai_coding_to_base_url() {
        let fixture = Provider::zai_coding("test_key");
        let actual = fixture.to_base_url();
        let expected = Url::parse("https://api.z.ai/api/coding/v1/").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_zai_coding_to_model_url() {
        let fixture = Provider::zai_coding("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/")
            .unwrap()
            .join("models")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_base_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.to_base_url();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_model_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/")
            .unwrap()
            .join("models")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_openai_to_base_url_and_model_url_same() {
        let fixture = Provider::openai("test_key");
        let base_url = fixture.to_base_url();
        let model_url = fixture.model_url();
        assert_eq!(base_url.join("models").unwrap(), model_url);
    }

    #[test]
    fn test_vertex_ai_global_location() {
        let fixture = Provider::vertex_ai("test_token", "forge-452914", "global").unwrap();
        let actual = fixture.to_base_url();
        let expected = Url::parse("https://aiplatform.googleapis.com/v1/projects/forge-452914/locations/global/endpoints/openapi/").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_vertex_ai_regular_location() {
        let fixture = Provider::vertex_ai("test_token", "test_project", "us-central1").unwrap();
        let actual = fixture.to_base_url();
        let expected = Url::parse("https://us-central1-aiplatform.googleapis.com/v1/projects/test_project/locations/us-central1/endpoints/openapi/").unwrap();
        assert_eq!(actual, expected);
    }
}
