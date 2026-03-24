/// Overwrites `base` with `other`.
pub fn overwrite<T>(base: &mut T, other: T) {
    *base = other;
}
