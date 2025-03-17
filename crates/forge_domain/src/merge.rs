pub mod std {
    pub fn overwrite<T>(base: &mut T, other: T) {
        *base = other;
    }
}

pub mod vec {

    pub use merge::vec::*;
    use merge::Merge;

    use super::Key;

    pub fn unify_by_key<T: Merge + Key>(base: &mut Vec<T>, other: Vec<T>) {
        for other_agent in other {
            if let Some(base_agent) = base.iter_mut().find(|a| a.key() == other_agent.key()) {
                // If the base contains an agent with the same Key, merge them
                base_agent.merge(other_agent);
            } else {
                // Otherwise, append the other agent to the base list
                base.push(other_agent);
            }
        }
    }
}

pub fn option<A>(base: &mut Option<A>, other: Option<A>) {
    if other.is_some() {
        *base = other;
    }
}

pub trait Key {
    type Id: Eq;
    fn key(&self) -> &Self::Id;
}
