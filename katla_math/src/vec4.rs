use core::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4(pub [f32; 4]);

impl Index<usize> for Vec4 {
    type Output = f32;

    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            3 => &self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Vec4"),
        }
    }
}

impl From<Vec4> for [f32; 4] {
    fn from(val: Vec4) -> Self {
        val.0
    }
}

impl Vec4 {
    #[inline]
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Vec4 {
        Vec4([x, y, z, 1.0])
    }

    #[inline]
    #[allow(dead_code)]
    pub fn add(_lhs: Vec4, _rhs: Vec4) -> Vec4 {
        Vec4([
            _lhs[0] + _rhs[0],
            _lhs[1] + _rhs[1],
            _lhs[2] + _rhs[2],
            _lhs[3] + _rhs[3],
        ])
    }

    #[inline]
    pub fn dot(a: &Vec4, b: &Vec4) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
    }
}
