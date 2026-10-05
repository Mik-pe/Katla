#[cfg(feature = "editor")]
mod tests {
    use katla_ecs::{Component, FieldKind, FieldMut, Inspect};

    #[derive(Component)]
    struct TestComponent {
        #[inspect(range(0.0, 100.0))]
        health: f32,
        #[inspect(skip)]
        internal_id: u32,
        name: String,
        #[inspect(color)]
        tint: [f32; 3],
        active: bool,
    }

    #[test]
    fn test_fields_count_and_skip() {
        let fields = TestComponent::fields();
        assert_eq!(fields.len(), 4); // internal_id is skipped
        assert!(fields.iter().all(|f| f.name != "internal_id"));
    }

    #[test]
    fn test_field_kinds() {
        let fields = TestComponent::fields();
        let health = fields.iter().find(|f| f.name == "health").unwrap();
        assert!(matches!(health.kind, FieldKind::Float));
        let tint = fields.iter().find(|f| f.name == "tint").unwrap();
        assert!(matches!(tint.kind, FieldKind::Color));
        let active = fields.iter().find(|f| f.name == "active").unwrap();
        assert!(matches!(active.kind, FieldKind::Bool));
    }

    #[test]
    fn test_field_constraints() {
        let fields = TestComponent::fields();
        let health = fields.iter().find(|f| f.name == "health").unwrap();
        assert_eq!(health.constraints.min, Some(0.0));
        assert_eq!(health.constraints.max, Some(100.0));
    }

    #[test]
    fn test_field_mut_f32() {
        let mut comp = TestComponent {
            health: 50.0,
            internal_id: 0,
            name: "test".into(),
            tint: [1.0, 1.0, 1.0],
            active: true,
        };
        if let Some(FieldMut::F32(val)) = comp.field_mut("health") {
            *val = 75.0;
        }
        assert_eq!(comp.health, 75.0);
    }

    #[test]
    fn test_field_mut_bool() {
        let mut comp = TestComponent {
            health: 50.0,
            internal_id: 0,
            name: "test".into(),
            tint: [1.0, 1.0, 1.0],
            active: true,
        };
        if let Some(FieldMut::Bool(val)) = comp.field_mut("active") {
            *val = false;
        }
        assert!(!comp.active);
    }

    #[test]
    fn test_field_mut_string() {
        let mut comp = TestComponent {
            health: 50.0,
            internal_id: 0,
            name: "test".into(),
            tint: [1.0, 1.0, 1.0],
            active: true,
        };
        if let Some(FieldMut::String(val)) = comp.field_mut("name") {
            val.push_str("_modified");
        }
        assert_eq!(comp.name, "test_modified");
    }

    #[test]
    fn test_display_name_generation() {
        let fields = TestComponent::fields();
        let health = fields.iter().find(|f| f.name == "health").unwrap();
        assert_eq!(health.display_name, "Health");
    }

    #[test]
    fn test_field_mut_unknown_for_array() {
        let mut comp = TestComponent {
            health: 50.0,
            internal_id: 0,
            name: "test".into(),
            tint: [1.0, 1.0, 1.0],
            active: true,
        };
        let result = comp.field_mut("tint");
        assert!(result.is_some());
        assert!(matches!(result, Some(FieldMut::Unknown(_))));
    }

    #[test]
    fn test_field_mut_nonexistent() {
        let mut comp = TestComponent {
            health: 50.0,
            internal_id: 0,
            name: "test".into(),
            tint: [1.0, 1.0, 1.0],
            active: true,
        };
        assert!(comp.field_mut("nonexistent").is_none());
    }

    #[test]
    fn test_type_name_correct() {
        let fields = TestComponent::fields();
        let health = fields.iter().find(|f| f.name == "health").unwrap();
        assert_eq!(health.type_name, "f32");
        let tint = fields.iter().find(|f| f.name == "tint").unwrap();
        assert_eq!(tint.type_name, "[f32; 3]");
    }

    #[derive(Component)]
    struct MultiWordField {
        emit_rate: f32,
        max_particle_count: u32,
    }

    #[test]
    fn test_multi_word_display_name() {
        let fields = MultiWordField::fields();
        let emit = fields.iter().find(|f| f.name == "emit_rate").unwrap();
        assert_eq!(emit.display_name, "Emit Rate");
        let count = fields
            .iter()
            .find(|f| f.name == "max_particle_count")
            .unwrap();
        assert_eq!(count.display_name, "Max Particle Count");
    }

    #[derive(Component)]
    struct CustomNameComponent {
        #[inspect(display_name = "Custom Health")]
        hp: f32,
    }

    #[test]
    fn test_custom_display_name() {
        let fields = CustomNameComponent::fields();
        let hp = fields.iter().find(|f| f.name == "hp").unwrap();
        assert_eq!(hp.display_name, "Custom Health");
    }
}
