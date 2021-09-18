use crate::Vec3;
use crate::Vec4;
use core::ops::Index;

/// Mat4 is considered a column-major matrix, constructed using 4 Vec4s
#[derive(Debug, Clone, PartialEq)]
pub struct Mat4(pub [Vec4; 4]);

impl Index<usize> for Mat4 {
    type Output = Vec4;

    fn index(&self, index: usize) -> &Vec4 {
        match index {
            0 => &self.0[0],
            1 => &self.0[1],
            2 => &self.0[2],
            3 => &self.0[3],
            _ => panic!("INDEXING OUT_OF_BOUNDS in Mat4"),
        }
    }
}

// impl Mul for Mat4 {
//     type Output = Self;

//     fn mul(self, rhs: Self) -> Self {
//         let row0 = self.extract_row(0);
//         let row1 = self.extract_row(1);
//         let row2 = self.extract_row(2);
//         let row3 = self.extract_row(3);

//         Mat4([
//             Vec4([
//                 Vec4::dot(&row0, &rhs[0]),
//                 Vec4::dot(&row1, &rhs[0]),
//                 Vec4::dot(&row2, &rhs[0]),
//                 Vec4::dot(&row3, &rhs[0]),
//             ]),
//             Vec4([
//                 Vec4::dot(&row0, &rhs[1]),
//                 Vec4::dot(&row1, &rhs[1]),
//                 Vec4::dot(&row2, &rhs[1]),
//                 Vec4::dot(&row3, &rhs[1]),
//             ]),
//             Vec4([
//                 Vec4::dot(&row0, &rhs[2]),
//                 Vec4::dot(&row1, &rhs[2]),
//                 Vec4::dot(&row2, &rhs[2]),
//                 Vec4::dot(&row3, &rhs[2]),
//             ]),
//             Vec4([
//                 Vec4::dot(&row0, &rhs[3]),
//                 Vec4::dot(&row1, &rhs[3]),
//                 Vec4::dot(&row2, &rhs[3]),
//                 Vec4::dot(&row3, &rhs[3]),
//             ]),
//         ])
//     }
// }
// impl Div for Mat4 {
//     type Output = Self;

//     fn div(&self, rhs: Self) -> Self {

//     }
// }

//Mat4 is considered a column-major matrix
impl Mat4 {
    pub fn new() -> Mat4 {
        Mat4([
            Vec4([1.0, 0.0, 0.0, 0.0]),
            Vec4([0.0, 1.0, 0.0, 0.0]),
            Vec4([0.0, 0.0, 1.0, 0.0]),
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ])
    }

    pub fn from_translation(pos: [f32; 3]) -> Mat4 {
        Mat4([
            Vec4([1.0, 0.0, 0.0, 0.0]),
            Vec4([0.0, 1.0, 0.0, 0.0]),
            Vec4([0.0, 0.0, 1.0, 0.0]),
            Vec4([pos[0], pos[1], pos[2], 1.0]),
        ])
    }

    //Internal functions which makes less sense
    pub fn extract_row(&self, index: usize) -> Vec4 {
        Vec4([
            self[0][index],
            self[1][index],
            self[2][index],
            self[3][index],
        ])
    }

    pub fn from_rotaxis(angle: &f32, axis: [f32; 3]) -> Mat4 {
        let cos_part = angle.cos();
        let sin_part = angle.sin();
        let one_sub_cos = 1.0 - cos_part;
        Mat4([
            Vec4([
                one_sub_cos * axis[0] * axis[0] + cos_part,
                one_sub_cos * axis[0] * axis[1] + sin_part * axis[2],
                one_sub_cos * axis[0] * axis[2] - sin_part * axis[1],
                0.0,
            ]),
            Vec4([
                one_sub_cos * axis[0] * axis[1] - sin_part * axis[2],
                one_sub_cos * axis[1] * axis[1] + cos_part,
                one_sub_cos * axis[1] * axis[2] + sin_part * axis[0],
                0.0,
            ]),
            Vec4([
                one_sub_cos * axis[0] * axis[2] + sin_part * axis[1],
                one_sub_cos * axis[1] * axis[2] - sin_part * axis[0],
                one_sub_cos * axis[2] * axis[2] + cos_part,
                0.0,
            ]),
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ])
    }

    #[allow(dead_code)]
    pub fn identity() -> Mat4 {
        Mat4([
            Vec4([1.0, 0.0, 0.0, 0.0]),
            Vec4([0.0, 1.0, 0.0, 0.0]),
            Vec4([0.0, 0.0, 1.0, 0.0]),
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ])
    }

    pub fn mul(&self, _rhs: &Mat4) -> Mat4 {
        let row0 = self.extract_row(0);
        let row1 = self.extract_row(1);
        let row2 = self.extract_row(2);
        let row3 = self.extract_row(3);

        Mat4([
            Vec4([
                Vec4::dot(&row0, &_rhs[0]),
                Vec4::dot(&row1, &_rhs[0]),
                Vec4::dot(&row2, &_rhs[0]),
                Vec4::dot(&row3, &_rhs[0]),
            ]),
            Vec4([
                Vec4::dot(&row0, &_rhs[1]),
                Vec4::dot(&row1, &_rhs[1]),
                Vec4::dot(&row2, &_rhs[1]),
                Vec4::dot(&row3, &_rhs[1]),
            ]),
            Vec4([
                Vec4::dot(&row0, &_rhs[2]),
                Vec4::dot(&row1, &_rhs[2]),
                Vec4::dot(&row2, &_rhs[2]),
                Vec4::dot(&row3, &_rhs[2]),
            ]),
            Vec4([
                Vec4::dot(&row0, &_rhs[3]),
                Vec4::dot(&row1, &_rhs[3]),
                Vec4::dot(&row2, &_rhs[3]),
                Vec4::dot(&row3, &_rhs[3]),
            ]),
        ])
    }

    pub fn create_ortho(bottom: f32, top: f32, left: f32, right: f32, near: f32, far: f32) -> Mat4 {
        Mat4([
            Vec4([
                2.0 / (right - left),
                0.0,
                0.0,
                -(right + left) / (right - left),
            ]),
            Vec4([
                0.0,
                2.0 / (top - bottom),
                0.0,
                -(top + bottom) / (top - bottom),
            ]),
            Vec4([0.0, 0.0, -2.0 / (far - near), -(far + near) / (far - near)]), // <-- Revise negativity
            Vec4([0.0, 0.0, 0.0, 1.0]),
        ])
    }

    pub fn create_proj(fov_angles: f32, aspect_ratio: f32, near: f32, far: f32) -> Mat4 {
        let fov_ratio = near * f32::tan(f32::to_radians(fov_angles) / 2.0);

        let r = aspect_ratio * fov_ratio;
        let l = -r;
        let t = fov_ratio;
        let b = -t;
        Mat4([
            Vec4([2f32 * near / (r - l), 0.0, 0.0, 0.0]),
            Vec4([0.0, 2f32 * near / (t - b), 0.0, 0.0]),
            Vec4([
                (r + l) / (r - l),
                (t + b) / (t - b),
                -(far + near) / (far - near),
                -1.0,
            ]),
            Vec4([0.0, 0.0, -2.0 * far * near / (far - near), 0.0]),
        ])
    }

    pub fn create_lookat(from: Vec3, to: Vec3, up: Vec3) -> Mat4 {
        let dir_fwd = (to - from).normalize();
        let dir_up = up.normalize();
        let dir_right = dir_fwd.cross(dir_up).normalize();
        let dir_up = dir_right.cross(dir_fwd).normalize();
        Mat4([
            Vec4([dir_right[0], dir_right[1], dir_right[2], 0.0]),
            Vec4([dir_up[0], dir_up[1], dir_up[2], 0.0]),
            Vec4([-dir_fwd[0], -dir_fwd[1], -dir_fwd[2], 0.0]),
            Vec4([from[0], from[1], from[2], 1.0]),
        ])
    }

    pub fn calc_det(&self) -> f32 {
        self[0][0] * self[1][1] * self[2][2] * self[3][3]
            + self[0][0] * self[1][2] * self[2][3] * self[3][1]
            + self[0][0] * self[1][3] * self[2][1] * self[3][2]
            + self[0][1] * self[1][0] * self[2][3] * self[3][2]
            + self[0][1] * self[1][2] * self[2][0] * self[3][3]
            + self[0][1] * self[1][3] * self[2][2] * self[3][0]
            + self[0][2] * self[1][0] * self[2][1] * self[3][3]
            + self[0][2] * self[1][1] * self[2][3] * self[3][0]
            + self[0][2] * self[1][3] * self[2][0] * self[3][1]
            + self[0][3] * self[1][0] * self[2][2] * self[3][1]
            + self[0][3] * self[1][1] * self[2][0] * self[3][2]
            + self[0][3] * self[1][2] * self[2][1] * self[3][0]
            - self[0][0] * self[1][1] * self[2][3] * self[3][2]
            - self[0][0] * self[1][2] * self[2][1] * self[3][3]
            - self[0][0] * self[1][3] * self[2][2] * self[3][1]
            - self[0][1] * self[1][0] * self[2][2] * self[3][3]
            - self[0][1] * self[1][2] * self[2][3] * self[3][0]
            - self[0][1] * self[1][3] * self[2][0] * self[3][2]
            - self[0][2] * self[1][0] * self[2][3] * self[3][1]
            - self[0][2] * self[1][1] * self[2][0] * self[3][3]
            - self[0][2] * self[1][3] * self[2][1] * self[3][0]
            - self[0][3] * self[1][0] * self[2][1] * self[3][2]
            - self[0][3] * self[1][1] * self[2][2] * self[3][0]
            - self[0][3] * self[1][2] * self[2][0] * self[3][1]
    }

    pub fn calc_inv_det(&self) -> f32 {
        1.0f32 / self.calc_det()
    }

    pub fn inverse(&self) -> Self {
        let inv_det = self.calc_inv_det();
        Self([
            Vec4([
                (self[1][1] * self[2][2] * self[3][3]
                    + self[1][2] * self[2][3] * self[3][1]
                    + self[1][3] * self[2][1] * self[3][2]
                    - self[1][1] * self[2][3] * self[3][2]
                    - self[1][2] * self[2][1] * self[3][3]
                    - self[1][3] * self[2][2] * self[3][1])
                    * inv_det,
                (self[0][1] * self[2][3] * self[3][2]
                    + self[0][2] * self[2][1] * self[3][3]
                    + self[0][3] * self[2][2] * self[3][1]
                    - self[0][1] * self[2][2] * self[3][3]
                    - self[0][2] * self[2][3] * self[3][1]
                    - self[0][3] * self[2][1] * self[3][2])
                    * inv_det,
                (self[0][1] * self[1][2] * self[3][3]
                    + self[0][2] * self[1][3] * self[3][1]
                    + self[0][3] * self[1][1] * self[3][2]
                    - self[0][1] * self[1][3] * self[3][2]
                    - self[0][2] * self[1][1] * self[3][3]
                    - self[0][3] * self[1][2] * self[3][1])
                    * inv_det,
                (self[0][1] * self[1][3] * self[2][2]
                    + self[0][2] * self[1][1] * self[2][3]
                    + self[0][3] * self[1][2] * self[2][1]
                    - self[0][1] * self[1][2] * self[2][3]
                    - self[0][2] * self[1][3] * self[2][1]
                    - self[0][3] * self[1][1] * self[2][2])
                    * inv_det,
            ]),
            Vec4([
                (self[1][0] * self[2][3] * self[3][2]
                    + self[1][2] * self[2][0] * self[3][3]
                    + self[1][3] * self[2][2] * self[3][0]
                    - self[1][0] * self[2][2] * self[3][3]
                    - self[1][2] * self[2][3] * self[3][0]
                    - self[1][3] * self[2][0] * self[3][2])
                    * inv_det,
                (self[0][0] * self[2][2] * self[3][3]
                    + self[0][2] * self[2][3] * self[3][0]
                    + self[0][3] * self[2][0] * self[3][2]
                    - self[0][0] * self[2][3] * self[3][2]
                    - self[0][2] * self[2][0] * self[3][3]
                    - self[0][3] * self[2][2] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][3] * self[3][2]
                    + self[0][2] * self[1][0] * self[3][3]
                    + self[0][3] * self[1][2] * self[3][0]
                    - self[0][0] * self[1][2] * self[3][3]
                    - self[0][2] * self[1][3] * self[3][0]
                    - self[0][3] * self[1][0] * self[3][2])
                    * inv_det,
                (self[0][0] * self[1][2] * self[2][3]
                    + self[0][2] * self[1][3] * self[2][0]
                    + self[0][3] * self[1][0] * self[2][2]
                    - self[0][0] * self[1][3] * self[2][2]
                    - self[0][2] * self[1][0] * self[2][3]
                    - self[0][3] * self[1][2] * self[2][0])
                    * inv_det,
            ]),
            Vec4([
                (self[1][0] * self[2][1] * self[3][3]
                    + self[1][1] * self[2][3] * self[3][0]
                    + self[1][3] * self[2][0] * self[3][1]
                    - self[1][0] * self[2][3] * self[3][1]
                    - self[1][1] * self[2][0] * self[3][3]
                    - self[1][3] * self[2][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[2][3] * self[3][1]
                    + self[0][1] * self[2][0] * self[3][3]
                    + self[0][3] * self[2][1] * self[3][0]
                    - self[0][0] * self[2][1] * self[3][3]
                    - self[0][1] * self[2][3] * self[3][0]
                    - self[0][3] * self[2][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[1][1] * self[3][3]
                    + self[0][1] * self[1][3] * self[3][0]
                    + self[0][3] * self[1][0] * self[3][1]
                    - self[0][0] * self[1][3] * self[3][1]
                    - self[0][1] * self[1][0] * self[3][3]
                    - self[0][3] * self[1][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][3] * self[2][1]
                    + self[0][1] * self[1][0] * self[2][3]
                    + self[0][3] * self[1][1] * self[2][0]
                    - self[0][0] * self[1][1] * self[2][3]
                    - self[0][1] * self[1][3] * self[2][0]
                    - self[0][3] * self[1][0] * self[2][1])
                    * inv_det,
            ]),
            Vec4([
                (self[1][0] * self[2][2] * self[3][1]
                    + self[1][1] * self[2][0] * self[3][2]
                    + self[1][2] * self[2][1] * self[3][0]
                    - self[1][0] * self[2][1] * self[3][2]
                    - self[1][1] * self[2][2] * self[3][0]
                    - self[1][2] * self[2][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[2][1] * self[3][2]
                    + self[0][1] * self[2][2] * self[3][0]
                    + self[0][2] * self[2][0] * self[3][1]
                    - self[0][0] * self[2][2] * self[3][1]
                    - self[0][1] * self[2][0] * self[3][2]
                    - self[0][2] * self[2][1] * self[3][0])
                    * inv_det,
                (self[0][0] * self[1][2] * self[3][1]
                    + self[0][1] * self[1][0] * self[3][2]
                    + self[0][2] * self[1][1] * self[3][0]
                    - self[0][0] * self[1][1] * self[3][2]
                    - self[0][1] * self[1][2] * self[3][0]
                    - self[0][2] * self[1][0] * self[3][1])
                    * inv_det,
                (self[0][0] * self[1][1] * self[2][2]
                    + self[0][1] * self[1][2] * self[2][0]
                    + self[0][2] * self[1][0] * self[2][1]
                    - self[0][0] * self[1][2] * self[2][1]
                    - self[0][1] * self[1][0] * self[2][2]
                    - self[0][2] * self[1][1] * self[2][0])
                    * inv_det,
            ]),
        ])
    }
}

impl Into<[[f32; 4]; 4]> for Mat4 {
    fn into(self) -> [[f32; 4]; 4] {
        let vec_arr = self.0;
        let f_arr = [
            vec_arr[0].into(),
            vec_arr[1].into(),
            vec_arr[2].into(),
            vec_arr[3].into(),
        ];
        f_arr
    }
}
