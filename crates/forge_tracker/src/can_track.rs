use std::env;

const LONG_ENV_FILTER_VAR_NAME: &str = "FORGE_TRACKER";

/// Version information
pub const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};

/// Checks if tracking is enabled
pub fn can_track() -> bool {
    let env_value = env::var(LONG_ENV_FILTER_VAR_NAME).ok();
    can_track_inner(Some(VERSION), env_value)
}

fn can_track_inner<V: AsRef<str>, E: AsRef<str>>(version: Option<V>, env_value: Option<E>) -> bool {
    let is_prod_build = if let Some(v) = version {
        let v_str = v.as_ref();
        !(v_str.contains("dev") || v_str.contains("0.1.0"))
    } else {
        true // If no version provided, assume prod
    };

    if let Some(value) = env_value {
        !value.as_ref().eq_ignore_ascii_case("false")
    } else {
        is_prod_build
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_enabled_true() {
        assert!(can_track_inner(Some("1.0.0"), Some("true")));
        assert!(can_track_inner(Some("0.1.0"), Some("true")));
        assert!(can_track_inner(Some("1.0.0"), Some("yes")));
        assert!(can_track_inner(Some("0.1.0"), Some("yes")));
    }

    #[test]
    fn usage_enabled_false() {
        assert!(!can_track_inner(Some("1.0.0"), Some("false")));
        assert!(!can_track_inner(Some("0.1.0"), Some("false")));
        assert!(!can_track_inner(Some("1.0.0"), Some("FALSE")));
        assert!(!can_track_inner(Some("0.1.0"), Some("False")));
    }

    #[test]
    fn usage_enabled_none_is_prod_true() {
        assert!(can_track_inner(Some("1.0.0"), None::<&str>));
    }

    #[test]
    fn usage_enabled_none_is_prod_false() {
        assert!(!can_track_inner(Some("0.1.0-dev"), None::<&str>));
        assert!(!can_track_inner(Some("1.0.0-dev"), None::<&str>));
        assert!(!can_track_inner(Some("0.1.0"), None::<&str>));
    }
}
