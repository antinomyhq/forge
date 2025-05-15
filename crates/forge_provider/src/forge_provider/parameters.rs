use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ParameterData {
    pub model: String,
    pub supported_parameters: Option<Vec<String>>,
    pub frequency_penalty_p10: Option<f64>,
    pub frequency_penalty_p50: Option<f64>,
    pub frequency_penalty_p90: Option<f64>,
    pub min_p_p10: Option<f64>,
    pub min_p_p50: Option<f64>,
    pub min_p_p90: Option<f64>,
    pub presence_penalty_p10: Option<f64>,
    pub presence_penalty_p50: Option<f64>,
    pub presence_penalty_p90: Option<f64>,
    pub repetition_penalty_p10: Option<f64>,
    pub repetition_penalty_p50: Option<f64>,
    pub repetition_penalty_p90: Option<f64>,
    pub temperature_p10: Option<f64>,
    pub temperature_p50: Option<f64>,
    pub temperature_p90: Option<f64>,
    pub top_a_p10: Option<f64>,
    pub top_a_p50: Option<f64>,
    pub top_a_p90: Option<f64>,
    pub top_k_p10: Option<f64>,
    pub top_k_p50: Option<f64>,
    pub top_k_p90: Option<f64>,
    pub top_p_p10: Option<f64>,
    pub top_p_p50: Option<f64>,
    pub top_p_p90: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ParameterResponse {
    pub data: ParameterData,
}
