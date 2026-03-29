/// A wrapper around `f32` that serializes to two decimal places, preventing
/// `toml_edit` from emitting noisy f64 bit-pattern approximations such as
/// `0.10000000149011612` instead of `0.1`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, schemars::JsonSchema, fake::Dummy)]
pub struct F32(pub f32);

impl From<f32> for F32 {
    fn from(v: f32) -> Self {
        Self(v)
    }
}

impl From<F32> for f32 {
    fn from(v: F32) -> Self {
        v.0
    }
}

impl serde::Serialize for F32 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let formatted: f64 = format!("{:.2}", self.0).parse().unwrap();
        serializer.serialize_f64(formatted)
    }
}

impl<'de> serde::Deserialize<'de> for F32 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(f32::deserialize(deserializer)?))
    }
}

/// A wrapper around `f64` that serializes to two decimal places, preventing
/// `toml_edit` from emitting noisy approximations such as
/// `0.20000000000000001` instead of `0.2`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, schemars::JsonSchema, fake::Dummy)]
pub struct F64(pub f64);

impl From<f64> for F64 {
    fn from(v: f64) -> Self {
        Self(v)
    }
}

impl From<F64> for f64 {
    fn from(v: F64) -> Self {
        v.0
    }
}

impl serde::Serialize for F64 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let formatted: f64 = format!("{:.2}", self.0).parse().unwrap();
        serializer.serialize_f64(formatted)
    }
}

impl<'de> serde::Deserialize<'de> for F64 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(f64::deserialize(deserializer)?))
    }
}
