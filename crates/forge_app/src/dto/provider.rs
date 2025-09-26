use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderUrl {
    OpenAI(Url),
    Anthropic(Url),
}
impl ProviderUrl {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderUrl::OpenAI(url) => url.as_str(),
            ProviderUrl::Anthropic(url) => url.as_str(),
        }
    }
}

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Provider {
    pub url: ProviderUrl,
    pub key: Option<String>,
}

impl Provider {
    pub fn forge(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::FORGE_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn openai(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::OPENAI_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn open_router(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::OPEN_ROUTER_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn requesty(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::REQUESTY_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn zai(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::ZAI_URL).unwrap()),
            key: Some(key.into()),
        }
    }
    pub fn zai_coding(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::ZAI_CODING_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn cerebras(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::CEREBRAS_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn xai(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::OpenAI(Url::parse(Provider::XAI_URL).unwrap()),
            key: Some(key.into()),
        }
    }

    pub fn anthropic(key: &str) -> Provider {
        Provider {
            url: ProviderUrl::Anthropic(Url::parse(Provider::ANTHROPIC_URL).unwrap()),
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
            url: ProviderUrl::OpenAI(Url::parse(&url)?),
            key: Some(key.into()),
        })
    }
}

impl Provider {
    pub const OPEN_ROUTER_URL: &str = "https://openrouter.ai/api/v1/";
    pub const REQUESTY_URL: &str = "https://router.requesty.ai/v1/";
    pub const XAI_URL: &str = "https://api.x.ai/v1/";
    pub const OPENAI_URL: &str = "https://api.openai.com/v1/";
    pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";
    pub const FORGE_URL: &str = "https://antinomy.ai/api/v1/";
    pub const ZAI_URL: &str = "https://api.z.ai/api/paas/v4/";
    pub const ZAI_CODING_URL: &str = "https://api.z.ai/api/coding/paas/v4/";
    pub const CEREBRAS_URL: &str = "https://api.cerebras.ai/v1/";

    /// Converts the provider to it's base URL
    pub fn to_base_url(&self) -> Url {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.clone(),
            ProviderUrl::Anthropic(url) => url.clone(),
        }
    }

    pub fn model_url(&self) -> Url {
        match &self.url {
            ProviderUrl::OpenAI(url) => {
                if self.is_zai_coding() {
                    let base_url = Url::parse(Provider::ZAI_URL).unwrap();
                    base_url.join("models").unwrap()
                } else {
                    url.join("models").unwrap()
                }
            }
            ProviderUrl::Anthropic(url) => url.join("models").unwrap(),
        }
    }

    pub fn is_forge(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::FORGE_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_open_router(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::OPEN_ROUTER_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_requesty(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::REQUESTY_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_zai(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::ZAI_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_zai_coding(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::ZAI_CODING_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_cerebras(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::CEREBRAS_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_xai(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::XAI_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_open_ai(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url.as_str().starts_with(Self::OPENAI_URL),
            ProviderUrl::Anthropic(_) => false,
        }
    }

    pub fn is_anthropic(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(_) => false,
            ProviderUrl::Anthropic(url) => url.as_str().starts_with(Self::ANTHROPIC_URL),
        }
    }

    pub fn is_vertex_ai(&self) -> bool {
        match &self.url {
            ProviderUrl::OpenAI(url) => url
                .as_str()
                .contains("aiplatform.googleapis.com/v1/projects/"),
            ProviderUrl::Anthropic(_) => false,
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
            url: ProviderUrl::OpenAI(Url::from_str("https://api.x.ai/v1/").unwrap()),
            key: Some(fixture.to_string()),
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_xai() {
        let fixture_xai = Provider::xai("key");
        assert!(fixture_xai.is_xai());

        let fixture_other = Provider::openai("key");
        assert!(!fixture_other.is_xai());
    }

    #[test]
    fn test_zai_coding_to_base_url() {
        let fixture = Provider::zai_coding("test_key");
        let actual = fixture.to_base_url();
        let expected = Url::parse(Provider::ZAI_CODING_URL).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_zai_coding_to_model_url() {
        let fixture = Provider::zai_coding("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse(Provider::ZAI_URL)
            .unwrap()
            .join("models")
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_base_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.to_base_url();
        let expected = Url::parse(Provider::ZAI_URL).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_model_url() {
        let fixture = Provider::zai("test_key");
        let actual = fixture.model_url();
        let expected = Url::parse(Provider::ZAI_URL)
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
