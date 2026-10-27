//! Bounded height index for variable-height native history rows.
use std::ops::Range;

const CAPACITY: usize = 10_000;

pub struct HeightIndex {
    heights: Vec<f32>,
    tree: Vec<f32>,
}

impl HeightIndex {
    pub fn new(len: usize, estimate: f32) -> Self {
        let mut index = Self {
            heights: Vec::with_capacity(CAPACITY),
            tree: vec![0.0; CAPACITY + 1],
        };
        for _ in 0..len.min(CAPACITY) {
            index.push(estimate);
        }
        index
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    #[cfg(test)]
    pub fn measured_heights(&self) -> &[f32] {
        &self.heights
    }

    pub fn push(&mut self, height: f32) {
        assert!(self.len() < CAPACITY);
        self.heights.push(0.0);
        self.set(self.len() - 1, height);
    }

    pub fn remove_first(&mut self) -> f32 {
        if self.heights.is_empty() {
            return 0.0;
        }
        let removed = self.heights.remove(0);
        let remaining = std::mem::take(&mut self.heights);
        self.tree.fill(0.0);
        for height in remaining {
            self.push(height);
        }
        removed
    }

    pub fn set(&mut self, row: usize, height: f32) -> bool {
        let height = if height.is_finite() {
            height.max(1.0)
        } else {
            1.0
        };
        let delta = height - self.heights[row];
        self.heights[row] = height;
        let mut i = row + 1;
        while i <= CAPACITY {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
        delta.abs() > 0.5
    }

    pub fn offset(&self, end: usize) -> f32 {
        let mut i = end.min(self.len());
        let mut result = 0.0;
        while i > 0 {
            result += self.tree[i];
            i &= i - 1;
        }
        result
    }

    pub fn total(&self) -> f32 {
        self.offset(self.len())
    }

    pub fn row_at(&self, y: f32) -> usize {
        if self.len() == 0 {
            return 0;
        }
        let mut index = 0;
        let mut sum = 0.0;
        let mut bit = 8192;
        while bit > 0 {
            let next = index + bit;
            if next <= self.len() && sum + self.tree[next] <= y.max(0.0) {
                index = next;
                sum += self.tree[next];
            }
            bit >>= 1;
        }
        index.min(self.len() - 1)
    }

    pub fn visible(&self, top: f32, height: f32, overscan: usize) -> Range<usize> {
        if self.len() == 0 {
            return 0..0;
        }
        let start = self.row_at(top).saturating_sub(overscan);
        let end =
            (self.row_at((top + height.max(0.0) - 0.01).max(top)) + 1 + overscan).min(self.len());
        start..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_finds_variable_height_rows_without_scanning_history() {
        let mut index = HeightIndex::new(10_000, 50.0);
        index.set(10, 500.0);
        assert_eq!(index.offset(11), 1000.0);
        assert_eq!(index.row_at(501.0), 10);
        assert_eq!(index.row_at(1001.0), 11);
        let range = index.visible(250_000.0, 600.0, 1);
        assert!(range.len() < 20);
        assert!(range.start > 4000);
    }

    #[test]
    fn insertions_and_height_updates_keep_prefix_offsets_correct() {
        let mut index = HeightIndex::new(2, 40.0);
        index.set(0, 100.0);
        index.push(40.0);
        assert_eq!(index.offset(2), 140.0);
        assert_eq!(index.total(), 180.0);
        assert_eq!(index.row_at(179.0), 2);
        assert_eq!(index.row_at(999.0), 2);
        assert_eq!(index.remove_first(), 100.0);
        assert_eq!(index.total(), 80.0);
        assert_eq!(index.row_at(41.0), 1);
    }

    #[test]
    fn empty_and_boundary_viewports_are_safe() {
        let empty = HeightIndex::new(0, 50.0);
        assert!(empty.visible(0.0, 500.0, 1).is_empty());
        let index = HeightIndex::new(3, 50.0);
        assert_eq!(index.visible(50.0, 49.0, 0), 1..2);
        assert_eq!(index.visible(150.0, 10.0, 0), 2..3);
    }
}
