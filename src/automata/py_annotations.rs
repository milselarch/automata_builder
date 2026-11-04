use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::automata::annotations::{AnnotationId, AnnotationMapping};

/// Python handle to the global (process-wide) rust `AnnotationMapping`,
/// which maps annotation strings to integer annotation IDs and back.
/// Annotation ID 0 is reserved for the empty annotation `""`.
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug, Default)]
pub struct PyAnnotationMapping {}

#[gen_stub_pymethods]
#[pymethods]
impl PyAnnotationMapping {
    #[new]
    pub fn new() -> Self {
        PyAnnotationMapping {}
    }
    /// ID of the empty annotation `""`
    #[classattr]
    pub const EMPTY_ID: u32 = AnnotationId::EMPTY.0;

    /// Insert an annotation string and return its annotation ID.
    /// Inserting an annotation that already exists returns the existing ID.
    #[staticmethod]
    pub fn insert(annotation: &str) -> u32 {
        AnnotationMapping::global_insert(annotation).0
    }
    /// Resolve an annotation ID to its annotation string.
    /// Raises KeyError if the annotation ID does not exist.
    #[staticmethod]
    pub fn resolve(annotation_id: u32) -> PyResult<String> {
        AnnotationMapping::global_resolve(AnnotationId(annotation_id))
            .map(|annotation| annotation.to_string())
            .ok_or_else(|| PyKeyError::new_err(format!(
                "annotation id {annotation_id} does not exist"
            )))
    }
    /// Annotation ID of an existing annotation string, or None
    /// if the annotation has not been inserted.
    #[staticmethod]
    pub fn get_id(annotation: &str) -> Option<u32> {
        AnnotationMapping::global_get_id(annotation).map(|id| id.0)
    }
    #[staticmethod]
    pub fn contains(annotation: &str) -> bool {
        AnnotationMapping::global_get_id(annotation).is_some()
    }
    /// Number of annotations in the mapping (including the empty annotation)
    #[staticmethod]
    pub fn num_annotations() -> usize {
        AnnotationMapping::global_len()
    }
}

define_stub_info_gatherer!(stub_info);
