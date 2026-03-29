/// A percentage value constrained to `[0.0, 1.0]` that serializes to two
/// decimal places, preventing floating-point noise in TOML output.
///
/// Validation is enforced at deserialization time, so any config file with an
/// out-of-range value is rejected with a descriptive error.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, schemars::JsonSchema)]
pub struct Percentage(f64);

impl Percentage {
    const MIN: f64 = 0.0;
    const MAX: f64 = 1.0;

    /// Construct a validated `Percentage`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `value` is outside `[0.0, 1.0]`.
    pub fn new(value: f64) -> Result<Self, String> {
        if value >= Self::MIN && value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(format!(
                "value must be between {} and {}, got {value}",
                Self::MIN,
                Self::MAX
            ))
        }
    }

    /// Returns the inner `f64` value.
    pub fn value(&self) -> f64 {
        self.0
    }
}

impl Default for Percentage {
    fn default() -> Self {
        Self(0.0)
    }
}

impl From<f64> for Percentage {
    fn from(v: f64) -> Self {
        Self(v)
    }
}

impl From<Percentage> for f64 {
    fn from(p: Percentage) -> Self {
        p.0
    }
}

impl fake::Dummy<fake::Faker> for Percentage {
    fn dummy_with_rng<R: fake::RngExt + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        use fake::Fake;
        Self((0.0f64..=1.0f64).fake_with_rng::<f64, R>(rng))
    }
}

impl serde::Serialize for Percentage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let formatted: f64 = format!("{:.2}", self.0).parse().unwrap();
        serializer.serialize_f64(formatted)
    }
}

impl<'de> serde::Deserialize<'de> for Percentage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_percentage_valid_range() {
        assert!(Percentage::new(0.0).is_ok());
        assert!(Percentage::new(0.5).is_ok());
        assert!(Percentage::new(1.0).is_ok());
    }

    #[test]
    fn test_percentage_rejects_out_of_range() {
        assert!(Percentage::new(-0.1).is_err());
        assert!(Percentage::new(1.1).is_err());
    }

    #[test]
    fn test_percentage_serializes_to_2dp() {
        #[derive(serde::Serialize)]
        struct Fixture {
            value: Percentage,
        }
        let fixture = Fixture { value: Percentage::new(0.2).unwrap() };
        let actual = toml_edit::ser::to_string_pretty(&fixture).unwrap();
        let expected = "value = 0.2\n";
        assert_eq!(actual, expected);
    }
}
