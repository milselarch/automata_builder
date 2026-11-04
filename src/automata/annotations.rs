use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::sync::{Arc, LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

/// Integer handle to an annotation string interned in an
/// [`AnnotationMapping`]. Products and transitions store this instead of the
/// annotation string itself so that they stay cheap to clone, hash and compare.
#[derive(
    Clone, Copy, Default, PartialEq, Eq, Hash,
    PartialOrd, Ord, Serialize, Deserialize
)]
pub struct AnnotationId(pub u32);

impl AnnotationId {
    /// ID of the empty annotation (`""`), which is pre-inserted into
    /// every [`AnnotationMapping`]
    pub const EMPTY: AnnotationId = AnnotationId(0);

    pub fn is_empty(&self) -> bool {
        *self == Self::EMPTY
    }
    /// Intern `annotation` in the global annotation mapping
    pub fn intern(annotation: &str) -> AnnotationId {
        AnnotationMapping::global_insert(annotation)
    }
    /// Resolve the annotation string from the global annotation mapping
    /// (unknown IDs resolve to the empty string)
    pub fn resolve(&self) -> Arc<str> {
        AnnotationMapping::global_resolve(*self).unwrap_or_else(|| Arc::from(""))
    }
}

impl Display for AnnotationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.pad(&self.resolve())
    }
}

impl Debug for AnnotationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnnotationId({}, {:?})", self.0, &*self.resolve())
    }
}

/// Bidirectional mapping between annotation strings and annotation IDs.
#[derive(Debug, Clone)]
pub struct AnnotationMapping {
    annotations: Vec<Arc<str>>,
    annotation_ids: HashMap<Arc<str>, AnnotationId>,
}

static GLOBAL_ANNOTATION_MAPPING: LazyLock<RwLock<AnnotationMapping>> =
    LazyLock::new(|| RwLock::new(AnnotationMapping::new()));

impl Default for AnnotationMapping {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotationMapping {
    pub fn new() -> Self {
        let mut mapping = AnnotationMapping {
            annotations: Vec::new(),
            annotation_ids: HashMap::new(),
        };
        let empty_id = mapping.insert("");
        debug_assert_eq!(empty_id, AnnotationId::EMPTY);
        mapping
    }

    /// Insert `annotation` and return its ID. Inserting an annotation that
    /// is already present returns the existing ID.
    pub fn insert(&mut self, annotation: &str) -> AnnotationId {
        if let Some(annotation_id) = self.annotation_ids.get(annotation) {
            return *annotation_id;
        }
        let raw_id = u32::try_from(self.annotations.len())
            .expect("exceeded maximum number of annotations");
        let annotation_id = AnnotationId(raw_id);
        let annotation: Arc<str> = Arc::from(annotation);
        self.annotations.push(annotation.clone());
        self.annotation_ids.insert(annotation, annotation_id);
        annotation_id
    }
    /// ID of `annotation` if it has been inserted
    pub fn get_id(&self, annotation: &str) -> Option<AnnotationId> {
        self.annotation_ids.get(annotation).copied()
    }
    /// Annotation string for `annotation_id` if it exists
    pub fn resolve(&self, annotation_id: AnnotationId) -> Option<&Arc<str>> {
        self.annotations.get(annotation_id.0 as usize)
    }
    pub fn len(&self) -> usize {
        self.annotations.len()
    }
    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    /// Process-wide annotation mapping shared by rust and python
    pub fn global() -> &'static RwLock<AnnotationMapping> {
        &GLOBAL_ANNOTATION_MAPPING
    }
    fn global_read() -> RwLockReadGuard<'static, AnnotationMapping> {
        Self::global().read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn global_write() -> RwLockWriteGuard<'static, AnnotationMapping> {
        Self::global().write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    pub fn global_insert(annotation: &str) -> AnnotationId {
        if let Some(annotation_id) = Self::global_read().get_id(annotation) {
            return annotation_id;
        }
        Self::global_write().insert(annotation)
    }
    pub fn global_get_id(annotation: &str) -> Option<AnnotationId> {
        Self::global_read().get_id(annotation)
    }
    pub fn global_resolve(annotation_id: AnnotationId) -> Option<Arc<str>> {
        Self::global_read().resolve(annotation_id).cloned()
    }
    pub fn global_len() -> usize {
        Self::global_read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_resolve() {
        let mut mapping = AnnotationMapping::new();
        assert_eq!(mapping.get_id(""), Some(AnnotationId::EMPTY));
        let foo_id = mapping.insert("foo");
        let bar_id = mapping.insert("bar");
        assert_ne!(foo_id, bar_id);
        assert_eq!(mapping.insert("foo"), foo_id);
        assert_eq!(mapping.resolve(foo_id).map(|s| &**s), Some("foo"));
        assert_eq!(mapping.resolve(bar_id).map(|s| &**s), Some("bar"));
        assert_eq!(mapping.resolve(AnnotationId(999)), None);
        assert_eq!(mapping.len(), 3);
    }

    #[test]
    fn global_mapping() {
        let annotation_id = AnnotationId::intern("GLOBAL_TEST_ANNOTATION");
        assert!(!annotation_id.is_empty());
        assert_eq!(AnnotationId::intern("GLOBAL_TEST_ANNOTATION"), annotation_id);
        assert_eq!(annotation_id.to_string(), "GLOBAL_TEST_ANNOTATION");
        assert_eq!(AnnotationId::EMPTY.to_string(), "");
        assert_eq!(format!("{:<4}|", AnnotationId::intern("ab")), "ab  |");
    }
}
