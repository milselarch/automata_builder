//! Python-facing wrapper around the global [`AnnotationMapping`].
//!
//! `PyAnnotationMapping` is a stateless alias class: every instance
//! operates on the same process-wide mapping, so the python reference
//! implementation and the rust core always agree on annotation ids.

use pyo3::{exceptions::{PyValueError, PyKeyError}, pyclass, pymethods, PyResult};
use crate::automata::annotation_mapping::{
    AnnotationMapping, global_get_or_insert, global_get_string, global_insert,
    global_lookup, global_len
};

use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

#[gen_stub_pyclass]
#[pyclass(subclass)]
#[derive(Clone, Debug)]
pub struct PyAnnotationMapping
{
    // the backing data is the process-wide global mapping; each instance
    // just borrows it per-call
    _inner: std::marker::PhantomData<AnnotationMapping>
}

#[gen_stub_pymethods]
#[pymethods]
impl PyAnnotationMapping {
    #[new]
    pub fn new() -> PyResult<Self> {
        Ok(PyAnnotationMapping {
            _inner: std::marker::PhantomData
        })
    }

    /// Register `annotation` as a new annotation string and return the
    /// fresh id assigned to it.
    ///
    /// Raises `ValueError` if the string is already registered (use
    /// `lookup` or `get_or_insert` to reuse an existing id).
    ///
    /// >>> m = PyAnnotationMapping()
    /// >>> m.insert('FOO')
    /// 1
    pub fn insert(&self, annotation: &str) -> PyResult<u32> {
        match global_insert(annotation) {
            Ok(id) => Ok(id),
            Err(err) => Err(PyValueError::new_err(err.to_string())),
        }
    }

    /// Return the already-registered id of `annotation`, or
    /// `0` (`NO_ANNOTATION`) if it was never registered.
    ///
    /// >>> m = PyAnnotationMapping()
    /// >>> m.insert('FOO')
    /// 1
    /// >>> m.lookup('FOO')
    /// 1
    /// >>> m.lookup('NOPE')
    /// 0
    pub fn lookup(&self, annotation: &str) -> u32 {
        global_lookup(annotation)
    }

    /// Return the already-registered id of `annotation`, or
    /// register it (reusing an existing registration if any) and
    /// return its id.
    ///
    /// >>> m = PyAnnotationMapping()
    /// >>> m.get_or_insert('BAR')
    /// 1
    /// >>> m.get_or_insert('BAR')
    /// 1
    pub fn get_or_insert(&self, annotation: &str) -> u32 {
        global_get_or_insert(annotation)
    }

    /// Resolve an id back to its annotation string.
    ///
    /// Raises `KeyError` for unknown ids.
    ///
    /// >>> m = PyAnnotationMapping()
    /// >>> id_foo = m.insert('FOO')
    /// >>> m.get_string(id_foo)
    /// 'FOO'
    pub fn get_string(&self, annotation_id: u32) -> PyResult<String> {
        global_get_string(annotation_id).ok_or_else(|| {
            PyKeyError::new_err(
                format!("unknown annotation id: {annotation_id}")
            )
        })
    }

    /// Number of annotation strings registered so far.
    ///
    /// >>> m = PyAnnotationMapping()
    /// >>> len(m)
    /// 0
    pub fn __len__(&self) -> usize {
        global_len()
    }

    pub fn __repr__(&self) -> String {
        format!("PyAnnotationMapping(num_annotations={})", global_len())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::automata::annotation_mapping::{AnnotationMapping, global_insert};

    #[test]
    fn insert_lookup() {
        let mut m = AnnotationMapping::new();
        let id = m.insert("FOO").unwrap();
        assert_eq!(m.lookup("FOO"), id);
        assert_eq!(m.lookup("NOPE"), 0);
        assert_eq!(m.get_string(id).unwrap(), "FOO");
        assert!(m.insert("FOO").is_err());
        assert_eq!(m.get_or_insert("FOO"), id);
    }

    #[test]
    fn global_roundtrip() {
        // note: global test may race on the id number under parallel
        // test execution, so capture whatever id it gets
        let id = global_insert("__GLOBAL_TEST_ANNOTATION__").unwrap();
        assert_eq!(global_lookup("__GLOBAL_TEST_ANNOTATION__"), id);
        assert_eq!(
            global_get_string(id).unwrap(),
            "__GLOBAL_TEST_ANNOTATION__"
        );
    }
}
