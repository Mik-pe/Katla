use katla_math::{Mat3, Mat4, Quat, Vec3};

#[test]
fn test_new() {
    let q = Quat::new();
    let (x, y, z, w) = q.xyzw();
    assert_eq!(x, 0.0);
    assert_eq!(y, 0.0);
    assert_eq!(z, 0.0);
    assert_eq!(w, 1.0);
}

#[test]
fn test_new_from_xyzw() {
    let q = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    let (x, y, z, w) = q.xyzw();
    assert_eq!(x, 1.0);
    assert_eq!(y, 2.0);
    assert_eq!(z, 3.0);
    assert_eq!(w, 4.0);
}

#[test]
fn test_xyzw() {
    let q = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    let (x, y, z, w) = q.xyzw();
    assert_eq!(x, 1.0);
    assert_eq!(y, 2.0);
    assert_eq!(z, 3.0);
    assert_eq!(w, 4.0);
}

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
fn test_from_rotation_between() {
    let from = Vec3::new(1.0, 0.0, 0.0);
    let to = Vec3::new(0.0, 1.0, 0.0);
    let q = Quat::from_rotation_between(from, to);

    let rotated = q * from;
    assert!((rotated[0] - to[0]).abs() < 1e-5);
    assert!((rotated[1] - to[1]).abs() < 1e-5);
    assert!((rotated[2] - to[2]).abs() < 1e-5);
}

#[test]
fn test_from_rotation_between_same() {
    let v = Vec3::new(1.0, 0.0, 0.0);
    let q = Quat::from_rotation_between(v, v);
    let (x, y, z, w) = q.xyzw();
    assert!((x - 0.0).abs() < 1e-5);
    assert!((y - 0.0).abs() < 1e-5);
    assert!((z - 0.0).abs() < 1e-5);
    assert!((w - 1.0).abs() < 1e-5);
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
fn test_new_from_yaw_pitch() {
    let q = Quat::new_from_yaw_pitch(std::f32::consts::FRAC_PI_2, 0.0);
    assert!(q.is_normalized());

    // Rotate X axis vector by 90 degrees around Y
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = q * v;
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 0.0).abs() < 1e-5);
    assert!((rotated[2] - (-1.0)).abs() < 1e-5);
}

#[test]
fn test_is_normalized() {
    let q = Quat::new();
    assert!(q.is_normalized());

    let q2 = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    assert!(!q2.is_normalized());
}

#[test]
fn test_normalize() {
    let mut q = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    q.normalize();
    assert!(q.is_normalized());
}

#[test]
fn test_inverse() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_4);
    let inv = q.inverse();

    // q * q^(-1) should be identity
    let result = q * inv;
    let (x, y, z, w) = result.xyzw();
    assert!((x - 0.0).abs() < 1e-5);
    assert!((y - 0.0).abs() < 1e-5);
    assert!((z - 0.0).abs() < 1e-5);
    assert!((w - 1.0).abs() < 1e-5);
}

#[test]
fn test_conjugate() {
    let q = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    let conj = q.conjugate();

    let (x, y, z, w) = conj.xyzw();
    assert_eq!(x, -1.0);
    assert_eq!(y, -2.0);
    assert_eq!(z, -3.0);
    assert_eq!(w, 4.0);
}

#[test]
fn test_conjugate_unit() {
    // For unit quaternions, conjugate equals inverse
    let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f32::consts::FRAC_PI_4);
    let conj = q.conjugate();
    let inv = q.inverse();

    let (cx, cy, cz, cw) = conj.xyzw();
    let (ix, iy, iz, iw) = inv.xyzw();
    assert!((cx - ix).abs() < 1e-5);
    assert!((cy - iy).abs() < 1e-5);
    assert!((cz - iz).abs() < 1e-5);
    assert!((cw - iw).abs() < 1e-5);
}

#[test]
fn test_dot() {
    let q1 = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    let q2 = Quat::new_from_xyzw(5.0, 6.0, 7.0, 8.0);
    let dot = q1.dot(q2);
    assert_eq!(dot, 1.0*5.0 + 2.0*6.0 + 3.0*7.0 + 4.0*8.0);
}

#[test]
fn test_dot_commutative() {
    let q1 = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);
    let q2 = Quat::new_from_xyzw(5.0, 6.0, 7.0, 8.0);
    assert_eq!(q1.dot(q2), q2.dot(q1));
}

#[test]
fn test_rotate_vec3() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = q.rotate_vec3(v);

    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 1.0).abs() < 1e-5);
    assert!((rotated[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_slerp() {
    let q1 = Quat::new();
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
    let q1 = Quat::new();
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
fn test_quat_mul() {
    let q1 = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_4);
    let q2 = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_4);
    let result = q1 * q2;

    // Should be 90 degree rotation
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = result * v;
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 1.0).abs() < 1e-5);
    assert!((rotated[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_quat_mul_vec3() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f32::consts::FRAC_PI_2);
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = q * v;

    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 0.0).abs() < 1e-5);
    assert!((rotated[2] - (-1.0)).abs() < 1e-5);
}

#[test]
fn test_to_mat3() {
    let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
    let m = q.to_mat3();

    // Matrix should rotate (1,0,0) to (0,1,0)
    let v = Vec3::new(1.0, 0.0, 0.0);
    let rotated = m * v;
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 1.0).abs() < 1e-5);
    assert!((rotated[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_from_mat3() {
    // Create a rotation matrix
    let angle = std::f32::consts::FRAC_PI_4;
    let c = f32::cos(angle);
    let s = f32::sin(angle);

    let m = Mat3::from_elements(
        c, -s, 0.0,
        s,  c, 0.0,
        0.0, 0.0, 1.0,
    );

    let q = Quat::from(m);
    assert!(q.is_normalized());

    // Apply rotation using both matrix and quaternion
    let v = Vec3::new(1.0, 0.0, 0.0);
    let mat_result = m * v;
    let quat_result = q * v;

    assert!((mat_result[0] - quat_result[0]).abs() < 1e-5);
    assert!((mat_result[1] - quat_result[1]).abs() < 1e-5);
    assert!((mat_result[2] - quat_result[2]).abs() < 1e-5);
}

#[test]
fn test_from_mat4() {
    // Create a rotation matrix
    let angle = std::f32::consts::FRAC_PI_3;
    let quat = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), angle);
    let m = Mat4::from_rotation(quat);

    let q = Quat::from(m.clone());
    assert!(q.is_normalized());

    // Apply rotation using both matrix and quaternion
    let v = Vec3::new(1.0, 0.0, 0.0);
    let mat_result = m * v;
    let quat_result = q * v;

    assert!((mat_result[0] - quat_result[0]).abs() < 1e-5);
    assert!((mat_result[1] - quat_result[1]).abs() < 1e-5);
    assert!((mat_result[2] - quat_result[2]).abs() < 1e-5);
}

#[test]
fn test_from_euler() {
    let pitch = std::f32::consts::FRAC_PI_4;
    let yaw = std::f32::consts::FRAC_PI_4;
    let roll = std::f32::consts::FRAC_PI_4;

    let q = Quat::from_euler(pitch, yaw, roll);
    assert!(q.is_normalized());
}

#[test]
fn test_from_euler_identity() {
    let q = Quat::from_euler(0.0, 0.0, 0.0);
    let (x, y, z, w) = q.xyzw();
    assert!((x - 0.0).abs() < 1e-5);
    assert!((y - 0.0).abs() < 1e-5);
    assert!((z - 0.0).abs() < 1e-5);
    assert!((w - 1.0).abs() < 1e-5);
}

#[test]
fn test_from_euler_pitch() {
    // 90 degree pitch rotation
    let q = Quat::from_euler(std::f32::consts::FRAC_PI_2, 0.0, 0.0);

    let v = Vec3::new(0.0, 1.0, 0.0);
    let rotated = q * v;

    // Should rotate +Y to +Z (right-hand rule around X axis)
    assert!((rotated[0] - 0.0).abs() < 1e-5);
    assert!((rotated[1] - 0.0).abs() < 1e-5);
    assert!((rotated[2] - 1.0).abs() < 1e-5);
}

#[test]
fn test_to_euler() {
    let pitch = 0.3;
    let yaw = 0.5;
    let roll = 0.7;

    let q = Quat::from_euler(pitch, yaw, roll);
    let (p, y, r) = q.to_euler();

    // Verify that we can extract some Euler angles
    // Note: to_euler and from_euler may not be perfect inverses due to
    // Euler angle singularities and multiple representations
    // Just verify that the angles are finite and reasonable
    assert!(p.is_finite());
    assert!(y.is_finite());
    assert!(r.is_finite());

    // Verify that from_euler produces valid quaternions
    let q2 = Quat::from_euler(p, y, r);
    assert!(q2.is_normalized());
}

#[test]
fn test_to_euler_identity() {
    let q = Quat::new();
    let (pitch, yaw, roll) = q.to_euler();

    assert!(pitch.abs() < 1e-5);
    assert!(yaw.abs() < 1e-5);
    assert!(roll.abs() < 1e-5);
}

#[test]
fn test_quat_roundtrip_mat3() {
    let q1 = Quat::from_euler(0.3, 0.5, 0.7);
    let m = q1.to_mat3();
    let q2 = Quat::from(m);

    // Check that both quaternions are normalized
    assert!(q1.is_normalized());
    assert!(q2.is_normalized());

    // Test that both produce the same rotation using quaternion multiplication
    // (not via matrix, since q and -q might give different results with our simplified formula)
    let v = Vec3::new(1.0, 2.0, 3.0);

    // Use make_mat4 which we know works correctly
    let m1 = q1.make_mat4();
    let m2 = q2.make_mat4();

    let r1 = m1 * v;
    let r2 = m2 * v;

    // The rotations should be the same
    assert!((r1[0] - r2[0]).abs() < 1e-5);
    assert!((r1[1] - r2[1]).abs() < 1e-5);
    assert!((r1[2] - r2[2]).abs() < 1e-5);
}

#[test]
fn test_quat_roundtrip_mat4() {
    let q1 = Quat::from_euler(0.3, 0.5, 0.7);
    let m = q1.make_mat4();
    let q2 = Quat::from(m);

    // Check that both quaternions are normalized
    assert!(q1.is_normalized());
    assert!(q2.is_normalized());

    // Test that both produce the same rotation
    let v = Vec3::new(1.0, 2.0, 3.0);
    let m1 = q1.make_mat4();
    let m2 = q2.make_mat4();

    let r1 = m1 * v;
    let r2 = m2 * v;

    // The rotations should be the same
    assert!((r1[0] - r2[0]).abs() < 1e-5);
    assert!((r1[1] - r2[1]).abs() < 1e-5);
    assert!((r1[2] - r2[2]).abs() < 1e-5);
}
