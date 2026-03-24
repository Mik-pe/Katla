//! Tests for the Component derive macro.
//!
//! These tests verify that the derive macro correctly implements
//! the Component trait for various struct types.

use katla_ecs::Component;
use std::any::{Any, TypeId};

// ============================================================================
// Test Components - Basic struct types
// ============================================================================

#[derive(Component)]
struct SimpleComponent {
    value: i32,
}

#[derive(Component)]
struct MultiFieldComponent {
    a: f32,
    b: String,
    c: Vec<i32>,
    d: bool,
}

#[derive(Component)]
#[allow(dead_code)]
struct GenericComponent<T: 'static> {
    data: T,
}

#[derive(Component)]
struct TupleStructComponent(i32, f32, String);

#[derive(Component)]
struct UnitComponent;

#[derive(Component)]
struct NestedComponent {
    inner: SimpleComponent,
    count: usize,
}

#[derive(Component)]
#[allow(dead_code)]
struct WithBoundsComponent<T: Clone + Send + Sync + 'static> {
    item: T,
}

// ============================================================================
// Unit Tests
// ============================================================================

#[test]
fn test_simple_component_implements_component() {
    let comp = SimpleComponent { value: 42 };
    assert_eq!(comp.value, 42);
}

#[test]
fn test_simple_component_satisfies_any_bound() {
    fn requires_any(_: &dyn Any) {}
    let comp = SimpleComponent { value: 100 };
    requires_any(&comp);
}

#[test]
fn test_multi_field_component() {
    let comp = MultiFieldComponent {
        a: 1.5,
        b: "test".to_string(),
        c: vec![1, 2, 3],
        d: true,
    };
    assert_eq!(comp.a, 1.5);
    assert_eq!(comp.b, "test");
    assert_eq!(comp.c, vec![1, 2, 3]);
    assert!(comp.d);
}

#[test]
fn test_generic_component() {
    let _comp_int = GenericComponent { data: 42i32 };

    let _comp_string = GenericComponent {
        data: "hello".to_string(),
    };
}

#[test]
fn test_tuple_struct_component() {
    let comp = TupleStructComponent(123, 456.7, "test".to_string());
    assert_eq!(comp.0, 123);
    assert_eq!(comp.1, 456.7);
    assert_eq!(comp.2, "test");
}

#[test]
fn test_unit_component() {
    let _comp = UnitComponent;
}

#[test]
fn test_nested_component() {
    let inner = SimpleComponent { value: 10 };
    let comp = NestedComponent { inner, count: 5 };
    assert_eq!(comp.inner.value, 10);
    assert_eq!(comp.count, 5);
}

#[test]
fn test_with_bounds_component() {
    let _comp = WithBoundsComponent { item: 42 };
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[derive(Component)]
struct ZeroSizedComponent;

#[test]
fn test_zero_sized_component() {
    let _comp = ZeroSizedComponent;
    assert!(std::mem::size_of::<ZeroSizedComponent>() == 0);
}

#[derive(Component)]
#[allow(dead_code)]
struct LargeArrayComponent {
    data: [u8; 1024],
}

#[test]
fn test_large_array_component() {
    let _comp = LargeArrayComponent { data: [0u8; 1024] };
}

#[derive(Component)]
#[allow(dead_code)]
struct OptionComponent {
    optional: Option<String>,
}

#[test]
fn test_option_field_component() {
    let comp_with_some = OptionComponent {
        optional: Some("value".to_string()),
    };
    assert!(comp_with_some.optional.is_some());

    let comp_with_none = OptionComponent { optional: None };
    assert!(comp_with_none.optional.is_none());
}

#[derive(Component)]
struct VecFieldComponent {
    items: Vec<i32>,
}

#[test]
fn test_vec_field_component_modification() {
    let mut comp = VecFieldComponent {
        items: vec![1, 2, 3],
    };
    comp.items.push(4);
    assert_eq!(comp.items, vec![1, 2, 3, 4]);
}

// ============================================================================
// Multiple Components Test
// ============================================================================

#[derive(Component)]
#[allow(dead_code)]
struct PositionComponent {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Component)]
#[allow(dead_code)]
struct VelocityComponent {
    dx: f32,
    dy: f32,
    dz: f32,
}

#[derive(Component)]
#[allow(dead_code)]
struct NameComponent {
    name: String,
}

#[test]
fn test_multiple_components_coexist() {
    let _pos = PositionComponent {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let _vel = VelocityComponent {
        dx: 0.5,
        dy: 0.5,
        dz: 0.5,
    };
    let _name = NameComponent {
        name: "Entity1".to_string(),
    };

    // Verify type IDs are distinct
    assert_ne!(
        TypeId::of::<PositionComponent>(),
        TypeId::of::<VelocityComponent>()
    );
    assert_ne!(
        TypeId::of::<PositionComponent>(),
        TypeId::of::<NameComponent>()
    );
    assert_ne!(
        TypeId::of::<VelocityComponent>(),
        TypeId::of::<NameComponent>()
    );
}

// ============================================================================
// Const Generics Test
// ============================================================================

#[derive(Component)]
#[allow(dead_code)]
struct ConstGenericComponent<const N: usize> {
    data: [i32; N],
}

#[test]
fn test_const_generic_component() {
    let _comp3 = ConstGenericComponent::<3> { data: [1, 2, 3] };

    let _comp5 = ConstGenericComponent::<5> {
        data: [1, 2, 3, 4, 5],
    };

    // Different const generics create different types
    assert_ne!(
        TypeId::of::<ConstGenericComponent<3>>(),
        TypeId::of::<ConstGenericComponent<5>>()
    );
}
