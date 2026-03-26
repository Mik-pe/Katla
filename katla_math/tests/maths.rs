use katla_math::{Mat4, Vec4};

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
    let mat = Mat4::identity();
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
