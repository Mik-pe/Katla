use std::time::Instant;

use approx::assert_abs_diff_eq;
use katla_math::{Mat4, Vec3, Vec4};

#[test]
fn test_memcpy() {
    let mut data = Vec::<i8>::new();
    const NUM_BYTES: usize = 1024 * 1024 * 16;
    for _ in 0..NUM_BYTES {
        data.push(0i8);
    }
    let mut other_data = Vec::<i8>::with_capacity(NUM_BYTES);
    unsafe {
        let before = Instant::now();
        libc::memcpy(other_data.as_mut_ptr() as _, data.as_ptr() as _, NUM_BYTES);
        // std::ptr::copy_nonoverlapping(data.as_ptr(), other_data.as_mut_ptr(), NUM_BYTES);
        let after = Instant::now();
        let time = (after - before);
        println!(
            "Duration was: {:.30} ({:.3} MB/s)",
            time.as_secs_f64(),
            (NUM_BYTES as f64) / (1024.0 * 1024.0) / time.as_secs_f64()
        );
    }
}

#[test]
fn test_cross() {
    use crate::Vec3;
    let x_axis = Vec3::new(1.0, 0.0, 0.0);
    let y_axis = Vec3::new(0.0, 1.0, 0.0);
    let cross_product = x_axis.cross(y_axis);
    assert_eq!(cross_product[0], 0.0);
    assert_eq!(cross_product[1], 0.0);
    assert_eq!(cross_product[2], 1.0);
}

#[test]
fn test_lerp() {
    let a = Vec3::new(0.0, 1.0, 0.0);
    let b = Vec3::new(1.0, 1.0, 0.0);
    let c = Vec3::lerp(a, b, 1.0);
    assert_abs_diff_eq!(c[0], b[0], epsilon = 0.0001);
    assert_abs_diff_eq!(c[1], b[1], epsilon = 0.0001);
    assert_abs_diff_eq!(c[2], b[2], epsilon = 0.0001);
}

#[test]
fn test_vec4_into() {
    let v = Vec4::from_xyz(1.0, 2.0, 3.0);
    {
        let v: [f32; 4] = v.into();
        assert_eq!(v, [1.0, 2.0, 3.0, 1.0]);
    }
}

#[test]
fn test_mat4_into() {
    let mat = Mat4::new();
    {
        let mat: [[f32; 4]; 4] = mat.into();
        assert_eq!(
            mat,
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0]
            ]
        );
    }
}
