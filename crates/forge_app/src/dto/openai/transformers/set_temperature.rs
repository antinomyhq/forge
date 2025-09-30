use forge_domain::Transformer;

use crate::dto::openai::Request;

/// A transformer that sets the temperature to a specific value
pub struct SetTemperature {
    temperature: f32,
}

impl SetTemperature {
    /// Creates a new SetTemperature transformer
    ///
    /// # Arguments
    /// * `temperature` - The temperature value to set (typically between 0.0
    ///   and 2.0)
    ///
    /// # Examples
    /// ```rust,ignore
    /// let transformer = SetTemperature::new(0.6);
    /// ```
    pub fn new(temperature: f32) -> Self {
        Self { temperature }
    }
}

impl Transformer for SetTemperature {
    type Value = Request;

    fn transform(&mut self, mut request: Self::Value) -> Self::Value {
        request.temperature = Some(self.temperature);
        request
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::ModelId;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_set_temperature() {
        // Fixture
        let mut transformer = SetTemperature::new(0.6);
        let request = Request::default()
            .model(ModelId::new("kimi-k2"))
            .temperature(1.0); // Initial temperature

        // Execute
        let actual = transformer.transform(request);

        // Expected: temperature should be set to 0.6
        assert_eq!(actual.temperature, Some(0.6));
    }

    #[test]
    fn test_set_temperature_no_existing_temperature() {
        // Fixture
        let mut transformer = SetTemperature::new(0.6);
        let request = Request::default().model(ModelId::new("kimi-k2"));

        // Execute
        let actual = transformer.transform(request);

        // Expected: temperature should be set to 0.6
        assert_eq!(actual.temperature, Some(0.6));
    }

    #[test]
    fn test_set_temperature_different_value() {
        // Fixture
        let mut transformer = SetTemperature::new(1.5);
        let request = Request::default();

        // Execute
        let actual = transformer.transform(request);

        // Expected: temperature should be set to 1.5
        assert_eq!(actual.temperature, Some(1.5));
    }
}
