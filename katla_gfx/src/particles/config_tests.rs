use super::{Align16Vec4, EmitterConfig, EmitterShape};

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn test_emitter_gpu_bytes_match_shader_members() {
    let config = EmitterConfig {
        position: [1.0, 2.0, 3.0],
        _pad_position: f32::NAN,
        shape: EmitterShape::Box,
        emit_rate: 4.0,
        base_lifetime: 5.0,
        lifetime_variation: 6.0,
        velocity_direction: [7.0, 8.0, 9.0],
        _pad_velocity: f32::NAN,
        velocity_magnitude: 10.0,
        velocity_cone_angle: 11.0,
        base_scale: 12.0,
        scale_variation: 13.0,
        color: [14.0, 15.0, 16.0, 17.0],
        color_variation: 18.0,
        color_end: Align16Vec4([19.0, 20.0, 21.0, 22.0]),
        shape_params: [23.0, 24.0, 25.0, 26.0],
        gravity: 27.0,
        turbulence_strength: 28.0,
        turbulence_frequency: 29.0,
        kill_all: 0x1234_5678,
        scale_end: 30.0,
        _pad2: [f32::NAN; 3],
    };
    let bytes = config.gpu_bytes();
    let module = naga::front::wgsl::parse_str(include_str!(
        "../../../resources/shaders/particles/common.wgsl"
    ))
    .unwrap();
    let ty = module
        .types
        .iter()
        .find_map(|(_, ty)| (ty.name.as_deref() == Some("EmitterConfig")).then_some(ty))
        .unwrap();
    let naga::TypeInner::Struct { members, span } = &ty.inner else {
        panic!("emitter configuration must be a WGSL struct");
    };
    assert_eq!(*span as usize, bytes.len());
    let expected = [
        ("position", floats(&config.position)),
        ("_pad_position", vec![0; 4]),
        ("shape", (config.shape as u32).to_le_bytes().to_vec()),
        ("emit_rate", floats(&[config.emit_rate])),
        ("base_lifetime", floats(&[config.base_lifetime])),
        ("lifetime_variation", floats(&[config.lifetime_variation])),
        ("velocity_direction", floats(&config.velocity_direction)),
        ("_pad_velocity", vec![0; 4]),
        ("velocity_magnitude", floats(&[config.velocity_magnitude])),
        ("velocity_cone_angle", floats(&[config.velocity_cone_angle])),
        ("base_scale", floats(&[config.base_scale])),
        ("scale_variation", floats(&[config.scale_variation])),
        ("color", floats(&config.color)),
        ("color_variation", floats(&[config.color_variation])),
        ("color_end", floats(&config.color_end.0)),
        ("shape_params", floats(&config.shape_params)),
        ("gravity", floats(&[config.gravity])),
        ("turbulence_strength", floats(&[config.turbulence_strength])),
        (
            "turbulence_frequency",
            floats(&[config.turbulence_frequency]),
        ),
        ("kill_all", config.kill_all.to_le_bytes().to_vec()),
        ("scale_end", floats(&[config.scale_end])),
        ("_pad2_0", vec![0; 4]),
        ("_pad2_1", vec![0; 4]),
        ("_pad2_2", vec![0; 4]),
    ];
    assert_eq!(members.len(), expected.len());
    for member in members {
        let name = member.name.as_deref().unwrap();
        let value = &expected.iter().find(|(field, _)| *field == name).unwrap().1;
        let offset = member.offset as usize;
        assert_eq!(&bytes[offset..offset + value.len()], value, "{name}");
    }
    assert_eq!(&bytes[84..96], &[0; 12]);
}

#[test]
fn test_emitter_gpu_shape_discriminants() {
    for (shape, value) in [
        (EmitterShape::Point, 0u32),
        (EmitterShape::Line, 1),
        (EmitterShape::Circle, 2),
        (EmitterShape::Sphere, 3),
        (EmitterShape::Box, 4),
    ] {
        let bytes = EmitterConfig {
            shape,
            ..EmitterConfig::default()
        }
        .gpu_bytes();
        assert_eq!(&bytes[16..20], &value.to_le_bytes());
    }
}
