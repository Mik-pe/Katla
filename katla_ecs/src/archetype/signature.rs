use std::any::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArchetypeId(pub u32);

impl ArchetypeId {
    pub const NULL: ArchetypeId = ArchetypeId(0);
}

pub type ComponentSignature = Vec<TypeId>;

pub fn signature_from_iter(type_ids: impl IntoIterator<Item = TypeId>) -> ComponentSignature {
    let mut sig: ComponentSignature = type_ids.into_iter().collect();
    sig.sort();
    sig.dedup();
    sig
}

pub fn signature_for_add(signature: &ComponentSignature, type_id: TypeId) -> ComponentSignature {
    let mut new_sig = signature.clone();
    if let Err(pos) = new_sig.binary_search(&type_id) {
        new_sig.insert(pos, type_id);
    }
    new_sig
}

pub fn signature_for_remove(signature: &ComponentSignature, type_id: TypeId) -> ComponentSignature {
    let mut new_sig = signature.clone();
    if let Ok(pos) = new_sig.binary_search(&type_id) {
        new_sig.remove(pos);
    }
    new_sig
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_from_iter_sorted() {
        let a = TypeId::of::<u32>();
        let b = TypeId::of::<i32>();
        let sig = signature_from_iter([b, a]);
        assert_eq!(sig.len(), 2);
        assert!(sig[0] < sig[1]);
    }

    #[test]
    fn test_signature_from_iter_dedup() {
        let a = TypeId::of::<i32>();
        let sig = signature_from_iter([a, a, a]);
        assert_eq!(sig.len(), 1);
    }

    #[test]
    fn test_signature_for_add_new() {
        let a = TypeId::of::<u32>();
        let b = TypeId::of::<i32>();
        let sig = signature_from_iter([a]);
        let added = signature_for_add(&sig, b);
        assert_eq!(added.len(), 2);
        assert!(added[0] < added[1]);
    }

    #[test]
    fn test_signature_for_add_existing() {
        let a = TypeId::of::<i32>();
        let sig = signature_from_iter([a]);
        let added = signature_for_add(&sig, a);
        assert_eq!(added.len(), 1);
    }

    #[test]
    fn test_signature_for_remove_existing() {
        let a = TypeId::of::<u32>();
        let b = TypeId::of::<i32>();
        let sig = signature_from_iter([a, b]);
        let removed = signature_for_remove(&sig, a);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], b);
    }

    #[test]
    fn test_signature_for_remove_missing() {
        let a = TypeId::of::<u32>();
        let b = TypeId::of::<i32>();
        let sig = signature_from_iter([a]);
        let removed = signature_for_remove(&sig, b);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], a);
    }

    #[test]
    fn test_archetype_id_equality() {
        assert_eq!(ArchetypeId(1), ArchetypeId(1));
        assert_ne!(ArchetypeId(1), ArchetypeId(2));
    }

    #[test]
    fn test_archetype_id_null() {
        assert_eq!(ArchetypeId::NULL, ArchetypeId(0));
    }
}
