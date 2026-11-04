//! Integer annotation ids shared by all transitions and products.
//!
//! Previously every `MultiTapeTransition` and `MultiTapeProduct` (as well as
//! their single-tape `Product` counterparts) carried a full `String`
//! annotation. Those strings are used for logging and debugging, but they
//! are also copied around a lot (product factories, clone derives, display).
//!
//! Now the annotation is stored as a cheap `u32` id. The single source of
//! truth mapping ids back to their original strings is the
//! [`AnnotationMapping`] below. A process-wide global mapping keeps the id
//! spaces consistent across the rust core and the python reference
//! implementation, and is exposed to python as `PyAnnotationMapping`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Cheap integer handle identifying an annotation.
pub type AnnotationID = u32;

/// Sentinel id meaning "no annotation".
pub const NO_ANNOTATION: AnnotationID = 0;

/// Error for [`AnnotationMapping::insert`] when the annotation
/// string is already registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationAlreadyExistsError {
    /// The annotation string that was already registered.
    pub annotation: String,
    /// The existing id of that annotation.
    pub existing_id: AnnotationID,
}

impl std::fmt::Display for AnnotationAlreadyExistsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "annotation '{}' is already registered with id {}",
            self.annotation, self.existing_id
        )
    }
}
impl std::error::Error for AnnotationAlreadyExistsError {}

#[derive(Clone, Debug)]
pub struct AnnotationMapping {
    /// `strings[id] == annotation string`, with `strings[0] == ""`
    /// (the implicit [`NO_ANNOTATION`] placeholder).
    strings: Vec<String>,
    /// `annotation string -> id`
    reverse: HashMap<String, AnnotationID>,
}

impl AnnotationMapping {
    /// Empty mapping. Note that id `0` is implicitly reserved for
    /// "no annotation" and always resolves to the empty string.
    pub fn new() -> Self {
        AnnotationMapping {
            strings: vec![String::new()],
            reverse: HashMap::new(),
        }
    }

    /// Register `annotation` as a new annotation and return its fresh id.
    ///
    /// Returns an error if `annotation` is already registered
    /// (re-registration must go through [`Self::lookup`] /
    /// [`Self::get_or_insert`] instead).
    pub fn insert(
        &mut self, annotation: &str
    ) -> Result<AnnotationID, AnnotationAlreadyExistsError> {
        if let Some(existing_id) = self.reverse.get(annotation) {
            return Err(AnnotationAlreadyExistsError {
                annotation: annotation.to_string(),
                existing_id: *existing_id,
            });
        }
        let id = self.strings.len() as AnnotationID;
        self.reverse.insert(annotation.to_string(), id);
        self.strings.push(annotation.to_string());
        Ok(id)
    }

    /// Return the existing id of `annotation`, or
    /// [`NO_ANNOTATION`] if it was never registered.
    pub fn lookup(&self, annotation: &str) -> AnnotationID {
        self.reverse.get(annotation).copied().unwrap_or(NO_ANNOTATION)
    }

    /// Return the existing id of `annotation`, or register it and
    /// return a fresh id.
    pub fn get_or_insert(&mut self, annotation: &str) -> AnnotationID {
        self.insert(annotation)
            .unwrap_or_else(|_| self.lookup(annotation))
    }

    /// Resolve an id back to its annotation string.
    ///
    /// Returns `None` for unknown ids (e.g. ids minted by a separate local
    /// [`AnnotationMapping`] instance).
    pub fn get_string(&self, id: AnnotationID) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_str())
    }

    /// `True` if `id` refers to a registered annotation.
    pub fn contains_id(&self, id: AnnotationID) -> bool {
        id < self.strings.len() as AnnotationID
    }

    /// Number of distinct annotations registered (including the
    /// implicit "no annotation" placeholder).
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Always `false`: the mapping implicitly contains the
    /// "no annotation" placeholder.
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl Default for AnnotationMapping {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// process-wide global mapping, shared between the rust core and the
// python reference implementation (via `PyAnnotationMapping`)
// ---------------------------------------------------------------------------

static GLOBAL_ANNOTATION_MAPPING: OnceLock<Mutex<AnnotationMapping>> = OnceLock::new();

/// The global mapping object (lazy-initialized).
fn global_mapping() -> &'static Mutex<AnnotationMapping> {
    GLOBAL_ANNOTATION_MAPPING.get_or_init(|| Mutex::new(AnnotationMapping::new()))
}

/// Lock the global mapping, recovering from a poisoned lock.
fn global() -> MutexGuard<'static, AnnotationMapping> {
    global_mapping()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Global version of [`AnnotationMapping::insert`].
pub fn global_insert(
    annotation: &str
) -> Result<AnnotationID, AnnotationAlreadyExistsError> {
    global().insert(annotation)
}

/// Global version of [`AnnotationMapping::lookup`].
pub fn global_lookup(annotation: &str) -> AnnotationID {
    global().lookup(annotation)
}

/// Global version of [`AnnotationMapping::get_or_insert`].
pub fn global_get_or_insert(annotation: &str) -> AnnotationID {
    global().get_or_insert(annotation)
}

/// Resolve an annotation id to its string in the global mapping.
pub fn global_get_string(id: AnnotationID) -> Option<String> {
    global().get_string(id).map(|s| s.to_string())
}

/// Number of annotations registered in the global mapping.
pub fn global_len() -> usize {
    global().len()
}

/// Human-readable representation of an annotation id, for
/// `Display` / logging / error messages.
///
/// Unknown ids render as `#<id>` (only possible for ids minted with
/// a separate local [`AnnotationMapping`] instance).
pub fn resolve_annotation(id: AnnotationID) -> String {
    global_get_string(id).unwrap_or_else(|| format!("#{id}"))
}
