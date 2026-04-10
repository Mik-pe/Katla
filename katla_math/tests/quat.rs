use katla_math::{Mat4, Quat, Vec3};

#[test]
fn test_from_axis_angle() {
    // 90 degree rotation around Z axis
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
    assert!(q.is_normalized());

    // Rotate X axis vector
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = q * v;
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 1.0).abs() < 1e-5);
    assert!((rotated[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_from_rotation_between_opposite() {
    let from = Vec3::new(1.0, 0.0, 0.0);
    let to = Vec3::new(-1.0, 0.0, 0.0);
    let q = Quat::from_rotation_between(from, to);
    assert!(q.is_normalized());

    // Should produce a 180 degree rotation
    let rotated = q * from;
    assert!((rotated[0] - to[0]).abs() < 1e-5);
    assert!((rotated[1] - to[1]).abs() < 1e-5);
    assert!((rotated[2] - to[2]).abs() < 1e-5);
}

#[test]
fn test_conjugate_unit_inverse() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_4);
    let inv = q.conjugate_unit();

    // q * q^(-1) should be identity
    let result = q * inv;
    let (x, y, z, w) = result.xyzw();
    assert!((x - 0.0).abs() < 1e-5);
    assert!((y - 0.0).abs() < 1e-5);
    assert!((z - 0.0).abs() < 1e-5);
    assert!((w - 1.0).abs() < 1e-5);
}

#[test]
fn test_conjugate_unit() {
    // For unit quaternions, conjugate equals inverse
    let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f32::consts::FRAC_PI_4);
    let conj = q.conjugate();
    let inv = q.conjugate_unit();

    let (cx, cy, cz, cw) = conj.xyzw();
    let (ix, iy, iz, iw) = inv.xyzw();
    assert!((cx - ix).abs() < 1e-5);
    assert!((cy - iy).abs() < 1e-5);
    assert!((cz - iz).abs() < 1e-5);
    assert!((cw - iw).abs() < 1e-5);
}

#[test]
fn test_slerp() {
    let q1 = Quat::identity();
    let q2 = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);

    // t=0 should give q1
    let result = Quat::slerp(q1, q2, 0.0);
    let (x, y, z, w) = result.xyzw();
    let (x1, y1, z1, w1) = q1.xyzw();
    assert!((x - x1).abs() < 1e-5);
    assert!((y - y1).abs() < 1e-5);
    assert!((z - z1).abs() < 1e-5);
    assert!((w - w1).abs() < 1e-5);

    // t=1 should give q2
    let result = Quat::slerp(q1, q2, 1.0);
    let (x, y, z, w) = result.xyzw();
    let (x2, y2, z2, w2) = q2.xyzw();
    assert!((x - x2).abs() < 1e-5);
    assert!((y - y2).abs() < 1e-5);
    assert!((z - z2).abs() < 1e-5);
    assert!((w - w2).abs() < 1e-5);

    // t=0.5 should give midpoint
    let result = Quat::slerp(q1, q2, 0.5);
    assert!(result.is_normalized());
}

#[test]
fn test_slerp_same() {
    let q1 = Quat::identity();
    let result = Quat::slerp(q1, q1, 0.5);
    let (x, y, z, w) = result.xyzw();
    let (x1, y1, z1, w1) = q1.xyzw();
    assert!((x - x1).abs() < 1e-5);
    assert!((y - y1).abs() < 1e-5);
    assert!((z - z1).abs() < 1e-5);
    assert!((w - w1).abs() < 1e-5);
}

#[test]
fn test_make_mat4() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
    let m = q.make_mat4();

    // Matrix should rotate (1,0,0) to (0,1,0)
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = m * v;
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 1.0).abs() < 1e-5);
    assert!((rotated[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_make_mat4_45_degree_rotation() {
    // Test with a 45-degree rotation where the matrix is NOT symmetric
    // This will catch row-major vs column-major bugs
    let angle = std::f32::consts::FRAC_PI_4; // 45 degrees
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), angle);
    let m = q.make_mat4();

    let cos_45 = f32::cos(angle);
    let sin_45 = f32::sin(angle);

    // Check column 0
    assert!(
        (m[0][0] - cos_45).abs() < 1e-5,
        "m[0][0] should be cos(45°)"
    );
    assert!(
        (m[0][1] - sin_45).abs() < 1e-5,
        "m[0][1] should be sin(45°)"
    );
    assert!((m[0][2] - 0.0).abs() < 1e-5);
    assert!((m[0][3] - 0.0).abs() < 1e-5);

    // Check column 1 (the crucial one - if transposed, sin and cos would swap)
    assert!(
        (m[1][0] - (-sin_45)).abs() < 1e-5,
        "m[1][0] should be -sin(45°)"
    );
    assert!(
        (m[1][1] - cos_45).abs() < 1e-5,
        "m[1][1] should be cos(45°)"
    );
    assert!((m[1][2] - 0.0).abs() < 1e-5);
    assert!((m[1][3] - 0.0).abs() < 1e-5);

    // Check column 2
    assert!((m[2][0] - 0.0).abs() < 1e-5);
    assert!((m[2][1] - 0.0).abs() < 1e-5);
    assert!((m[2][2] - 1.0).abs() < 1e-5);
    assert!((m[2][3] - 0.0).abs() < 1e-5);

    // Check column 3
    assert!((m[3][0] - 0.0).abs() < 1e-5);
    assert!((m[3][1] - 0.0).abs() < 1e-5);
    assert!((m[3][2] - 0.0).abs() < 1e-5);
    assert!((m[3][3] - 1.0).abs() < 1e-5);
}

#[test]
fn test_quat_roundtrip_mat3() {
    let q1 = Quat::from_euler(0.3, 0.5, 0.7);
    let m = q1.to_mat3();
    let q2 = Quat::from(m);

    assert!(q1.is_normalized());
    assert!(q2.is_normalized());

    // Test that both produce the same rotation
    let v = Vec3::new(1.0, 2.0, 3.0);

    let m1 = q1.make_mat4();
    let m2 = q2.make_mat4();

    let r1 = m1 * v;
    let r2 = m2 * v;

    assert!((r1[0] - r2[0]).abs() < 1e-5);
    assert!((r1[1] - r2[1]).abs() < 1e-5);
    assert!((r1[2] - r2[2]).abs() < 1e-5);
}

#[test]
fn test_quat_roundtrip_mat4() {
    let q1 = Quat::from_euler(0.3, 0.5, 0.7);
    let m = q1.make_mat4();
    let q2 = Quat::from(m);

    assert!(q1.is_normalized());
    assert!(q2.is_normalized());

    let v = Vec3::new(1.0, 2.0, 3.0);
    let m1 = q1.make_mat4();
    let m2 = q2.make_mat4();

    let r1 = m1 * v;
    let r2 = m2 * v;

    assert!((r1[0] - r2[0]).abs() < 1e-5);
    assert!((r1[1] - r2[1]).abs() < 1e-5);
    assert!((r1[2] - r2[2]).abs() < 1e-5);
}

#[test]
fn test_from_trs_matrix_elements() {
    let translation = Vec3::new(1.0, 2.0, 3.0);
    let rotation = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_4);
    let scale = Vec3::new(2.0, 3.0, 4.0);

    let m = Mat4::from_trs(translation, rotation, scale);

    // Translation column (column 3)
    assert!((m[3][0] - 1.0).abs() < 1e-5, "Translation X should be 1.0");
    assert!((m[3][1] - 2.0).abs() < 1e-5, "Translation Y should be 2.0");
    assert!((m[3][2] - 3.0).abs() < 1e-5, "Translation Z should be 3.0");
    assert!((m[3][3] - 1.0).abs() < 1e-5, "Translation W should be 1.0");

    // Check that scale is applied (check the length of basis vectors)
    let col0_len = (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt();
    assert!(
        (col0_len - 2.0).abs() < 1e-5,
        "Column 0 length should be 2.0 (scale X)"
    );

    let col1_len = (m[1][0] * m[1][0] + m[1][1] * m[1][1] + m[1][2] * m[1][2]).sqrt();
    assert!(
        (col1_len - 3.0).abs() < 1e-5,
        "Column 1 length should be 3.0 (scale Y)"
    );

    let col2_len = (m[2][0] * m[2][0] + m[2][1] * m[2][1] + m[2][2] * m[2][2]).sqrt();
    assert!(
        (col2_len - 4.0).abs() < 1e-5,
        "Column 2 length should be 4.0 (scale Z)"
    );
}

#[test]
fn test_from_trs_decompose_recompose() {
    let original = Mat4::from_trs(
        Vec3::new(1.0, 2.0, 3.0),
        Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.5),
        Vec3::new(2.0, 2.0, 2.0),
    );

    let decomposed = original.decompose();
    let recomposed = decomposed.make_mat4();

    for col in 0..4 {
        for row in 0..4 {
            assert!(
                (original[col][row] - recomposed[col][row]).abs() < 1e-4,
                "Matrix element [{}, {}] mismatch after decompose/recompose: {} vs {}",
                col,
                row,
                original[col][row],
                recomposed[col][row]
            );
        }
    }
}

#[test]
fn test_quat_to_mat3_preserves_orthogonality() {
    let q = Quat::from_euler(0.3, 0.7, 0.5);
    let m = q.to_mat3();
    let col0 = Vec3::new(m[0][0], m[1][0], m[2][0]);
    let col1 = Vec3::new(m[0][1], m[1][1], m[2][1]);
    let col2 = Vec3::new(m[0][2], m[1][2], m[2][2]);

    // All columns must be unit length
    assert!((col0.length() - 1.0).abs() < 1e-5);
    assert!((col1.length() - 1.0).abs() < 1e-5);
    assert!((col2.length() - 1.0).abs() < 1e-5);

    // All pairs must be orthogonal
    assert!(col0.dot(col1).abs() < 1e-5);
    assert!(col0.dot(col2).abs() < 1e-5);
    assert!(col1.dot(col2).abs() < 1e-5);
}

#[test]
fn test_slerp_long_path() {
    let q1 = Quat::from_euler(0.0, 0.0, 0.0);
    let q2 = Quat::from_euler(3.1, 0.0, 0.0); // nearly opposite
    let mid = Quat::slerp(q1, q2, 0.5);
    // The result should be a valid unit quaternion (no NaN/Inf)
    let (w, x, y, z) = mid.xyzw();
    assert!(w.is_finite() && x.is_finite() && y.is_finite() && z.is_finite());
    // Midpoint should be roughly halfway in rotation
    assert!(mid.is_normalized());
}
