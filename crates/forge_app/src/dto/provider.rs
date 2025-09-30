use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};
use url::Url;

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Display, EnumString)]
pub enum ProviderId {
    Forge,
    #[strum(serialize = "OpenAI")]
    OpenAI,
    #[strum(serialize = "OpenRouter")]
    OpenRouter,
    Requesty,
    Zai,
    #[strum(serialize = "ZaiCoding")]
    ZaiCoding,
    Cerebras,
    Xai,
    Anthropic,
    #[strum(serialize = "VertexAi")]
    VertexAi,
}

pub const OPEN_ROUTER_URL: &str = "https://openrouter.ai/api/v1/";
pub const REQUESTY_URL: &str = "https://router.requesty.ai/v1/";
pub const XAI_URL: &str = "https://api.x.ai/v1/";
pub const OPENAI_URL: &str = "https://api.openai.com/v1/";
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";
pub const FORGE_URL: &str = "https://antinomy.ai/api/v1/";
pub const ZAI_URL: &str = "https://api.z.ai/api/paas/v4/";
pub const ZAI_CODING_URL: &str = "https://api.z.ai/api/coding/paas/v4/";
pub const CEREBRAS_URL: &str = "https://api.cerebras.ai/v1/";

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
            url: Url::parse(FORGE_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn openai(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse(OPENAI_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn open_router(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenRouter,
            response: ProviderResponse::OpenAI,
            url: Url::parse(OPEN_ROUTER_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn requesty(key: &str) -> Provider {
        Provider {
            id: ProviderId::Requesty,
            response: ProviderResponse::OpenAI,
            url: Url::parse(REQUESTY_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn zai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Zai,
            response: ProviderResponse::OpenAI,
            url: Url::parse(ZAI_URL).unwrap(),
            key: Some(key.into()),
        }
    }
    pub fn zai_coding(key: &str) -> Provider {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse(ZAI_CODING_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn cerebras(key: &str) -> Provider {
        Provider {
            id: ProviderId::Cerebras,
            response: ProviderResponse::OpenAI,
            url: Url::parse(CEREBRAS_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn xai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse(XAI_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn anthropic(key: &str) -> Provider {
        Provider {
            id: ProviderId::Anthropic,
            response: ProviderResponse::Anthropic,
            url: Url::parse(ANTHROPIC_URL).unwrap(),
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
                    let base_url = Url::parse(ZAI_URL).unwrap();
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
        let expected = Url::parse(ZAI_CODING_URL).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_zai_coding_to_model_url() {
        let fixture = Provider::zai_coding("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse(ZAI_URL).unwrap().join("models").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_base_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.to_base_url();
        let expected = Url::parse(ZAI_URL).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_model_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse(ZAI_URL).unwrap().join("models").unwrap();
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
