//! A minimal 2D array, standing in for the numpy arrays the Python uses.
//!
//! Indexing is `(y, x)` — row first — exactly like numpy, so ported index
//! expressions read the same as the originals.

use std::ops::{Index, IndexMut};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix<T> {
    data: Vec<T>,
    width: usize,
    height: usize,
}

impl<T: Clone + Default> Matrix<T> {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![T::default(); width * height],
            width,
            height,
        }
    }
}

impl<T: Clone> Matrix<T> {
    pub fn filled(width: usize, height: usize, value: T) -> Self {
        Self {
            data: vec![value; width * height],
            width,
            height,
        }
    }

    pub fn from_vec(data: Vec<T>, width: usize, height: usize) -> Self {
        assert_eq!(data.len(), width * height, "matrix data length mismatch");
        Self {
            data,
            width,
            height,
        }
    }

    /// Build from nested rows, as the Python tests' literal arrays do.
    pub fn from_rows(rows: Vec<Vec<T>>) -> Self {
        let height = rows.len();
        let width = if height > 0 { rows[0].len() } else { 0 };
        let mut data = Vec::with_capacity(width * height);
        for row in rows {
            assert_eq!(row.len(), width, "ragged rows");
            data.extend(row);
        }
        Self {
            data,
            width,
            height,
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// numpy's `.shape` — `(height, width)`.
    pub fn shape(&self) -> (usize, usize) {
        (self.height, self.width)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    pub fn get(&self, y: usize, x: usize) -> &T {
        &self.data[y * self.width + x]
    }

    #[inline]
    pub fn set(&mut self, y: usize, x: usize, value: T) {
        let w = self.width;
        self.data[y * w + x] = value;
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    pub fn into_vec(self) -> Vec<T> {
        self.data
    }

    pub fn row(&self, y: usize) -> &[T] {
        &self.data[y * self.width..(y + 1) * self.width]
    }

    pub fn row_mut(&mut self, y: usize) -> &mut [T] {
        let w = self.width;
        &mut self.data[y * w..(y + 1) * w]
    }

    pub fn map<U: Clone, F: FnMut(&T) -> U>(&self, mut f: F) -> Matrix<U> {
        Matrix {
            data: self.data.iter().map(&mut f).collect(),
            width: self.width,
            height: self.height,
        }
    }

    /// numpy's `a.repeat(factor, 0).repeat(factor, 1)`.
    pub fn repeat(&self, factor: usize) -> Matrix<T> {
        let width = self.width * factor;
        let height = self.height * factor;
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push(self.data[(y / factor) * self.width + (x / factor)].clone());
            }
        }
        Matrix {
            data,
            width,
            height,
        }
    }
}

impl<T> Index<(usize, usize)> for Matrix<T> {
    type Output = T;
    /// `m[(y, x)]`
    #[inline]
    fn index(&self, (y, x): (usize, usize)) -> &T {
        &self.data[y * self.width + x]
    }
}

impl<T> IndexMut<(usize, usize)> for Matrix<T> {
    #[inline]
    fn index_mut(&mut self, (y, x): (usize, usize)) -> &mut T {
        let w = self.width;
        &mut self.data[y * w + x]
    }
}
