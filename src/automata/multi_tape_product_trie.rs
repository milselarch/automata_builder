use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use indexmap::{IndexMap, IndexSet};

use crate::automata::terms_multitape::{MultiTapeProduct, MultiTapeTerm};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TrieError {
    /// Two tries with different (non-blank) offsets were merged together.
    OffsetMismatch { left: i64, right: i64 },
}

impl fmt::Display for TrieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrieError::OffsetMismatch { left, right } => {
                write!(f, "offset mismatch: {} vs {}", left, right)
            }
        }
    }
}
impl std::error::Error for TrieError {}

/// Allocation-free equivalent of `MultiTapeProduct::is_satisfiable_with`.
///
/// For every term in `other_terms`, if `self_terms` contains a term at the
/// same `(position, tape_no)`, then its cell state must match.
/// When `self_terms` contains multiple terms for the same `(position, tape_no)`,
/// the last one wins (mirroring the `HashMap` based implementation).
pub fn terms_satisfiable_with(
    self_terms: &[MultiTapeTerm], other_terms: &[MultiTapeTerm],
) -> bool {
    for other_term in other_terms.iter() {
        let (other_tape_no, other_cell_state) = other_term.state;
        let existing = self_terms.iter().rev().find(|term| {
            term.position == other_term.position && term.state.0 == other_tape_no
        });
        if let Some(existing_term) = existing {
            if existing_term.state.1 != other_cell_state {
                return false;
            }
        }
    }
    true
}

/// Sorting key to sort terms by position in zigzag order
/// (lower absolute value first, then positive before negative offsets,
/// matching the python `zigzag_sort_key`): 0, 1, -1, 2, -2, ...
pub fn zigzag_term_cmp(a: &MultiTapeTerm, b: &MultiTapeTerm) -> Ordering {
    let key_a = (a.position.unsigned_abs(), a.position < 0);
    let key_b = (b.position.unsigned_abs(), b.position < 0);
    key_a.cmp(&key_b).then_with(|| a.cmp(b))
}

/// Port of the python `OffsetGroupedTerms`:
/// (sorted) product terms that all share the same position offset
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct OffsetGroupedTerms {
    pub terms: Vec<MultiTapeTerm>,
    pub offset: Option<i64>,
}

impl OffsetGroupedTerms {
    pub fn new(terms: Vec<MultiTapeTerm>, offset: i64) -> Self {
        debug_assert!(terms.windows(2).all(|pair| pair[0] <= pair[1]));
        debug_assert!(terms.iter().all(|term| term.position == offset));
        Self { terms, offset: Some(offset) }
    }

    pub fn blank() -> Self {
        Self { terms: vec![], offset: None }
    }

    pub fn is_blank(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn to_product(&self) -> MultiTapeProduct {
        MultiTapeProduct::new(self.terms.clone())
    }

    pub fn satisfiable_with(&self, other: &OffsetGroupedTerms) -> bool {
        if self.offset != other.offset {
            return false;
        }
        terms_satisfiable_with(&self.terms, &other.terms)
    }
}

pub fn offset_group_path_to_product(group_path: &[OffsetGroupedTerms]) -> MultiTapeProduct {
    let flat_terms: Vec<MultiTapeTerm> = group_path
        .iter()
        .flat_map(|group| group.terms.iter().cloned())
        .collect();
    MultiTapeProduct::new(flat_terms)
}

/// Port of the python `MultiTapeProductTrie`:
/// a trie of product terms nested from smallest to largest term offset
/// (in zigzag order, i.e. 0, 1, -1, 2, -2, ...)
///
/// Sub-tries are reference counted so that they can be cheaply shared
/// between tries (the python implementation shares sub-tries as well),
/// and so that tries can be shared across threads.
#[derive(Debug, Clone, Default)]
pub struct MultiTapeProductTrie {
    pub offset: Option<i64>,
    pub end_products: IndexSet<MultiTapeProduct>,
    pub next_groups: IndexMap<OffsetGroupedTerms, Arc<MultiTapeProductTrie>>,
}

impl MultiTapeProductTrie {
    pub fn new(offset: Option<i64>) -> Self {
        Self { offset, end_products: IndexSet::new(), next_groups: IndexMap::new() }
    }

    pub fn spawn_root() -> Self {
        Self::new(None)
    }

    /// whether this trie has any end products
    pub fn has_end_product(&self) -> bool {
        !self.end_products.is_empty()
    }

    /// whether any nested trie exists
    pub fn has_nested_products(&self) -> bool {
        !self.next_groups.is_empty()
    }

    /// whether this trie or any nested trie contains an end product
    pub fn has_products(&self) -> bool {
        self.has_end_product() || self.has_nested_products()
    }

    /// Python `__or__`
    pub fn union(&self, other: &MultiTapeProductTrie) -> Result<Self, TrieError> {
        let offset = match (self.offset, other.offset) {
            (None, offset) => offset,
            (offset, None) => offset,
            (Some(left), Some(right)) => {
                if left != right {
                    return Err(TrieError::OffsetMismatch { left, right });
                }
                Some(left)
            }
        };

        let mut end_products = self.end_products.clone();
        for product in other.end_products.iter() {
            end_products.insert(product.clone());
        }

        let mut next_groups = self.next_groups.clone();
        for (group, other_trie) in other.next_groups.iter() {
            let merged = match next_groups.get(group) {
                Some(existing) => Arc::new(existing.union(other_trie)?),
                None => other_trie.clone(),
            };
            next_groups.insert(group.clone(), merged);
        }

        Ok(Self { offset, end_products, next_groups })
    }

    /// Python `merge`: (t0 | t1) | t2 | ...
    pub fn merge(tries: &[Arc<MultiTapeProductTrie>]) -> Result<Self, TrieError> {
        assert!(!tries.is_empty());
        let mut merged: MultiTapeProductTrie = (*tries[0]).clone();
        for trie in tries[1..].iter() {
            merged = merged.union(trie)?;
        }
        Ok(merged)
    }

    /// Insert an offset group -> trie exclusion into own trie exclusions.
    pub fn insert(
        &mut self, offset_group: &OffsetGroupedTerms, trie_exclusion: &Arc<MultiTapeProductTrie>,
    ) -> Result<(), TrieError> {
        let offset = offset_group.offset;
        if offset.is_none() {
            return Ok(());
        }

        for (next_offset_group, next_trie) in self.next_groups.iter_mut() {
            if next_offset_group.offset == offset {
                continue;
            }
            *next_trie = Arc::new(next_trie.union(trie_exclusion)?);
        }

        let merged = match self.next_groups.get(offset_group) {
            Some(existing) => Arc::new(existing.union(trie_exclusion)?),
            None => trie_exclusion.clone(),
        };
        self.next_groups.insert(offset_group.clone(), merged);
        Ok(())
    }

    /// Offsets of the next groups, in ascending order
    /// (`None` i.e. blank groups first)
    pub fn get_next_offsets(&self) -> Vec<Option<i64>> {
        let offsets: BTreeSet<Option<i64>> =
            self.next_groups.keys().map(|group| group.offset).collect();
        offsets.into_iter().collect()
    }

    /// Remove all nested groups (at every level) whose offset is in `offsets`.
    /// Returns `None` if nothing would be removed.
    fn _without_nested_offsets(&self, offsets: &BTreeSet<i64>) -> Option<Self> {
        let mut changed = false;
        let mut next_groups: IndexMap<OffsetGroupedTerms, Arc<MultiTapeProductTrie>> =
            IndexMap::with_capacity(self.next_groups.len());

        for (offset_group, next_trie) in self.next_groups.iter() {
            if let Some(offset) = offset_group.offset {
                if offsets.contains(&offset) {
                    changed = true;
                    continue;
                }
            }
            match next_trie._without_nested_offsets(offsets) {
                Some(new_trie) => {
                    changed = true;
                    next_groups.insert(offset_group.clone(), Arc::new(new_trie));
                }
                None => {
                    next_groups.insert(offset_group.clone(), next_trie.clone());
                }
            }
        }

        if !changed {
            return None;
        }
        Some(Self {
            offset: self.offset,
            end_products: self.end_products.clone(),
            next_groups,
        })
    }

    pub fn without_nested_offsets(&self, offsets: &BTreeSet<i64>) -> Self {
        if offsets.is_empty() {
            return self.clone();
        }
        self._without_nested_offsets(offsets).unwrap_or_else(|| self.clone())
    }

    pub fn match_offset_groups_for(
        &self, target_group: &OffsetGroupedTerms,
    ) -> Vec<&OffsetGroupedTerms> {
        self.next_groups
            .keys()
            .filter(|offset_group| offset_group.satisfiable_with(target_group))
            .collect()
    }

    pub fn advance_exclusions(
        &self, source_offset_group: &OffsetGroupedTerms,
    ) -> Result<MultiTapeProductTrie, TrieError> {
        let source_offset = source_offset_group
            .offset
            .expect("source offset group cannot be blank");

        /*
        It is possible for multiple offset groups to match the source group,
        e.g. (D(0,0,0), D(0,1,0)), (D(0,1,0), D(0,3,0)) and (D(0,3,0),)
        could all match some source group.
        If we only went along one of them, a pre-existing product
        matching our current product along another matching offset group
        could be missed; hence the need to merge all product exclusions
        across matching offset groups
        */
        let matching_exclusions: Vec<Arc<MultiTapeProductTrie>> = self
            .next_groups
            .iter()
            .filter(|(offset_group, _)| offset_group.satisfiable_with(source_offset_group))
            .map(|(_, trie)| trie.clone())
            .collect();

        let mut next_exclusions = if matching_exclusions.is_empty() {
            MultiTapeProductTrie::new(None)
        } else {
            MultiTapeProductTrie::merge(&matching_exclusions)?
        };

        for (offset_group, adjacent_exclusions) in self.next_groups.iter() {
            if offset_group.offset == Some(source_offset) {
                continue;
            }
            next_exclusions.insert(offset_group, adjacent_exclusions)?;
        }

        let offsets = BTreeSet::from([source_offset]);
        Ok(match next_exclusions._without_nested_offsets(&offsets) {
            Some(trie) => trie,
            None => next_exclusions,
        })
    }

    /// terms sorted by position in zigzag order, without duplicates
    pub fn build_term_path(terms: &[MultiTapeTerm]) -> Vec<MultiTapeTerm> {
        let unique_terms: IndexSet<MultiTapeTerm> = terms.iter().cloned().collect();
        let mut term_path: Vec<MultiTapeTerm> = unique_terms.into_iter().collect();
        term_path.sort_by(zigzag_term_cmp);
        term_path
    }

    /// terms grouped by offset with groups sorted by offset in zigzag order
    pub fn create_group_path(terms: &[MultiTapeTerm]) -> Vec<OffsetGroupedTerms> {
        let sorted_terms = Self::build_term_path(terms);
        let mut offset_groups: Vec<OffsetGroupedTerms> = Vec::new();
        let mut grouped_terms: Vec<MultiTapeTerm> = Vec::new();
        let mut prev_offset: i64 = 0;

        for term in sorted_terms.into_iter() {
            if !grouped_terms.is_empty() && term.position != prev_offset {
                offset_groups.push(OffsetGroupedTerms::new(
                    std::mem::take(&mut grouped_terms), prev_offset,
                ));
            }
            prev_offset = term.position;
            grouped_terms.push(term);
        }
        if !grouped_terms.is_empty() {
            offset_groups.push(OffsetGroupedTerms::new(grouped_terms, prev_offset));
        }
        offset_groups
    }

    pub fn group_path_from_product(product: &MultiTapeProduct) -> Vec<OffsetGroupedTerms> {
        Self::create_group_path(&product._terms)
    }

    fn _insert_group_path(&mut self, group_path: &[OffsetGroupedTerms], product: MultiTapeProduct) {
        let Some((current_group, next_groups)) = group_path.split_first() else {
            self.end_products.insert(product);
            return;
        };
        let next_trie = self
            .next_groups
            .entry(current_group.clone())
            .or_insert_with(|| Arc::new(MultiTapeProductTrie::new(current_group.offset)));
        Arc::make_mut(next_trie)._insert_group_path(next_groups, product);
    }

    pub fn insert_product(&mut self, product: &MultiTapeProduct) {
        let group_path = Self::group_path_from_product(product);
        self._insert_group_path(&group_path, product.clone());
    }

    fn _has_group_path(&self, group_path: &[OffsetGroupedTerms]) -> bool {
        let Some((current_group, next_groups)) = group_path.split_first() else {
            return self.has_end_product();
        };
        match self.next_groups.get(current_group) {
            Some(next_trie) => next_trie._has_group_path(next_groups),
            None => false,
        }
    }

    pub fn has_product(&self, product: &MultiTapeProduct) -> bool {
        let group_path = Self::group_path_from_product(product);
        self._has_group_path(&group_path)
    }

    fn _load_matching_products(
        &self, group_path: &[OffsetGroupedTerms], all_products: &mut IndexSet<MultiTapeProduct>,
    ) {
        for product in self.end_products.iter() {
            all_products.insert(product.clone());
        }
        let Some((current_group, next_groups)) = group_path.split_first() else {
            return;
        };
        for (offset_group, next_trie) in self.next_groups.iter() {
            if offset_group.satisfiable_with(current_group) {
                next_trie._load_matching_products(next_groups, all_products);
            }
        }
    }

    pub fn load_matching_products_for_terms(
        &self, terms: &[MultiTapeTerm],
    ) -> IndexSet<MultiTapeProduct> {
        let group_path = Self::create_group_path(terms);
        let mut all_products = IndexSet::new();
        self._load_matching_products(&group_path, &mut all_products);
        all_products
    }

    pub fn load_matching_products(&self, product: &MultiTapeProduct) -> IndexSet<MultiTapeProduct> {
        self.load_matching_products_for_terms(&product._terms)
    }

    fn _search_all_products(&self, all_products: &mut IndexSet<MultiTapeProduct>) {
        for product in self.end_products.iter() {
            all_products.insert(product.clone());
        }
        for next_trie in self.next_groups.values() {
            next_trie._search_all_products(all_products);
        }
    }

    pub fn search_all_products(&self) -> IndexSet<MultiTapeProduct> {
        let mut all_products = IndexSet::new();
        self._search_all_products(&mut all_products);
        all_products
    }

    pub fn search_all_nested_products(&self) -> IndexSet<MultiTapeProduct> {
        let mut all_products = IndexSet::new();
        for next_trie in self.next_groups.values() {
            next_trie._search_all_products(&mut all_products);
        }
        all_products
    }

    fn _search_all_offsets(&self, all_offsets: &mut BTreeSet<i64>) {
        if let Some(offset) = self.offset {
            all_offsets.insert(offset);
        }
        for next_trie in self.next_groups.values() {
            next_trie._search_all_offsets(all_offsets);
        }
    }

    pub fn search_all_offsets(&self) -> BTreeSet<i64> {
        let mut all_offsets = BTreeSet::new();
        self._search_all_offsets(&mut all_offsets);
        all_offsets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(position: i64, tape_no: u16, state: u32) -> MultiTapeTerm {
        MultiTapeTerm::new(position, (tape_no, state), false)
    }

    #[test]
    fn group_path_is_zigzag_sorted() {
        let terms = vec![d(1, 0, 1), d(-1, 0, 2), d(0, 1, 3), d(0, 0, 4), d(1, 0, 1)];
        let group_path = MultiTapeProductTrie::create_group_path(&terms);
        let offsets: Vec<Option<i64>> = group_path.iter().map(|g| g.offset).collect();
        assert_eq!(offsets, vec![Some(0), Some(1), Some(-1)]);
        assert_eq!(group_path[0].terms, vec![d(0, 0, 4), d(0, 1, 3)]);
        assert_eq!(group_path[2].terms, vec![d(-1, 0, 2)]);
    }

    #[test]
    fn load_matching_products() {
        let mut trie = MultiTapeProductTrie::spawn_root();
        let product_a = MultiTapeProduct::new(vec![d(0, 0, 1), d(1, 0, 2)]);
        let product_b = MultiTapeProduct::new(vec![d(0, 0, 1)]);
        let product_c = MultiTapeProduct::new(vec![d(0, 0, 3)]);
        trie.insert_product(&product_a);
        trie.insert_product(&product_b);
        trie.insert_product(&product_c);

        assert!(trie.has_product(&product_a));
        let matches = trie.load_matching_products(
            &MultiTapeProduct::new(vec![d(0, 0, 1), d(0, 1, 5), d(1, 0, 2)])
        );
        assert!(matches.contains(&product_a));
        assert!(matches.contains(&product_b));
        assert!(!matches.contains(&product_c));
        assert_eq!(trie.search_all_offsets(), BTreeSet::from([0, 1]));
    }

    #[test]
    fn union_offset_mismatch() {
        let left = MultiTapeProductTrie::new(Some(0));
        let right = MultiTapeProductTrie::new(Some(1));
        assert!(left.union(&right).is_err());
        assert!(left.union(&MultiTapeProductTrie::new(None)).is_ok());
    }
}
