use std::collections::HashMap;
use std::hash::Hash;

/// Trait for grouping a collection of items by a key extracted from each item.
pub trait GroupByKey<K, V> {
    fn group_by_key(value: Vec<V>, key_fn: impl Fn(&V) -> K) -> HashMap<K, Vec<V>>;
}

impl<K: Hash + Eq + Clone, V: Ord + Clone> GroupByKey<K, V> for Vec<V> {
    fn group_by_key(values: Vec<V>, key_fn: impl Fn(&V) -> K) -> HashMap<K, Vec<V>> {
        values
            .iter()
            .fold(
                HashMap::<K, Vec<&V>>::new(),
                |mut acc: HashMap<_, _>, chunk| {
                    acc.entry(key_fn(chunk)).or_default().push(chunk);
                    acc
                },
            )
            .into_iter()
            .map(|(key, mut chunks)| {
                chunks.sort();

                (key.clone(), chunks.into_iter().cloned().collect::<Vec<_>>())
            })
            .collect::<HashMap<_, Vec<_>>>()
    }
}
