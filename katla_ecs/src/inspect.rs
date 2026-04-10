use std::any::Any;

use crate::Component;

/// Metadata describing a single field of a component.
#[derive(Clone)]
pub struct FieldInfo {
    pub name: &'static str,
    pub display_name: &'static str,
    pub type_name: &'static str,
    pub kind: FieldKind,
    pub constraints: FieldConstraints,
}

#[derive(Clone, Default)]
pub enum FieldKind {
    #[default]
    Unknown,
    Float,
    Int,
    Bool,
    String,
    Color,
    Struct,
    Enum {
        variants: &'static [&'static str],
    },
    Vec,
    EntityRef,
}

#[derive(Clone, Default)]
pub struct FieldConstraints {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub speed: Option<f32>,
    pub skip: bool,
}

/// Mutable accessor for a field value.
pub enum FieldMut<'a> {
    F32(&'a mut f32),
    F64(&'a mut f64),
    I32(&'a mut i32),
    U32(&'a mut u32),
    I64(&'a mut i64),
    U64(&'a mut u64),
    Bool(&'a mut bool),
    String(&'a mut String),
    Unknown(&'a mut dyn Any),
}

/// Trait for runtime component reflection.
///
/// Provides metadata about component fields and mutable access to their values.
/// Automatically implemented by `#[derive(Component)]` when the `editor` feature is enabled.
pub trait Inspect: Component {
    fn fields() -> Vec<FieldInfo>
    where
        Self: Sized;
    fn field_mut(&mut self, name: &str) -> Option<FieldMut<'_>>;
}
