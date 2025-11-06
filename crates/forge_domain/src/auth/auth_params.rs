/// Authorization URL parameters
#[derive(Debug, Clone)]
pub struct AuthCodeParams {
    pub auth_url: String,
    pub state: String,
    pub code_verifier: Option<String>,
}
