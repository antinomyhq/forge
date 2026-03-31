/// Unconditionally overwrites `base` with `other`.
///
/// Intended for use as a [`merge`] strategy on non-`Option` fields.
pub(crate) fn overwrite<T>(base: &mut T, other: T) {
    *base = other;
}

/// Overwrites `base` with `other` only when `other` is `Some`.
///
/// Intended for use as a [`merge`] strategy on `Option<T>` fields.
pub(crate) fn overwrite_some<T>(base: &mut Option<T>, other: Option<T>) {
    if other.is_some() {
        *base = other;
    }
}
