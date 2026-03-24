//! Tests for the Component derive macro.
//!
//! These tests verify that the derive macro correctly implements
//! the Component trait for various struct types.

use katla_ecs::Component;
use std::any::Any;

// ============================================================================
// Test Components - Basic struct types
// ============================================================================

// Note: Components with lifetime parameters require special handling
// since Component extends Any which requires 'static.
// We'll test these separately with static lifetime bounds.

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
fn test_simple_component_as_any() {
    let comp = SimpleComponent { value: 100 };

    // Test as_any returns reference that can be downcast
    let any: &dyn Any = comp.as_any();
    assert!(any.is::<SimpleComponent>());

    let downcast = any.downcast_ref::<SimpleComponent>();
    assert!(downcast.is_some());
    assert_eq!(downcast.unwrap().value, 100);
}

#[test]
fn test_simple_component_as_any_mut() {
    let mut comp = SimpleComponent { value: 50 };

    // Test as_any_mut returns mutable reference that can be downcast
    let any_mut: &mut dyn Any = comp.as_any_mut();
    assert!(any_mut.is::<SimpleComponent>());

    let downcast = any_mut.downcast_mut::<SimpleComponent>();
    assert!(downcast.is_some());

    // Modify through downcast
    downcast.unwrap().value = 999;
    assert_eq!(comp.value, 999);
}

#[test]
fn test_multi_field_component() {
    let comp = MultiFieldComponent {
        a: 1.5,
        b: "test".to_string(),
        c: vec![1, 2, 3],
        d: true,
    };

    let any = comp.as_any();
    assert!(any.is::<MultiFieldComponent>());

    let downcast = any.downcast_ref::<MultiFieldComponent>();
    assert!(downcast.is_some());
    let comp_ref = downcast.unwrap();

    assert_eq!(comp_ref.a, 1.5);
    assert_eq!(comp_ref.b, "test");
    assert_eq!(comp_ref.c, vec![1, 2, 3]);
    assert!(comp_ref.d);
}

#[test]
fn test_generic_component() {
    let comp_int = GenericComponent { data: 42 };
    assert!(comp_int.as_any().is::<GenericComponent<i32>>());

    let comp_string = GenericComponent {
        data: "hello".to_string(),
    };
    assert!(comp_string.as_any().is::<GenericComponent<String>>());
}

#[test]
fn test_tuple_struct_component() {
    let comp = TupleStructComponent(123, 456.7, "test".to_string());
    assert!(comp.as_any().is::<TupleStructComponent>());

    let any = comp.as_any();
    let downcast = any.downcast_ref::<TupleStructComponent>().unwrap();
    assert_eq!(downcast.0, 123);
    assert_eq!(downcast.1, 456.7);
    assert_eq!(downcast.2, "test");
}

#[test]
fn test_unit_component() {
    let comp = UnitComponent;
    assert!(comp.as_any().is::<UnitComponent>());
}

#[test]
fn test_nested_component() {
    let inner = SimpleComponent { value: 10 };
    let comp = NestedComponent { inner, count: 5 };

    assert!(comp.as_any().is::<NestedComponent>());

    let any = comp.as_any();
    let downcast = any.downcast_ref::<NestedComponent>().unwrap();
    assert_eq!(downcast.inner.value, 10);
    assert_eq!(downcast.count, 5);
}

#[test]
fn test_with_bounds_component() {
    let comp = WithBoundsComponent { item: 42 };
    assert!(comp.as_any().is::<WithBoundsComponent<i32>>());

    let any = comp.as_any();
    let downcast = any.downcast_ref::<WithBoundsComponent<i32>>().unwrap();
    assert_eq!(downcast.item, 42);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[derive(Component)]
struct ZeroSizedComponent;

#[test]
fn test_zero_sized_component() {
    let comp = ZeroSizedComponent;
    assert!(comp.as_any().is::<ZeroSizedComponent>());
    assert!(std::mem::size_of::<ZeroSizedComponent>() == 0);
}

#[derive(Component)]
#[allow(dead_code)]
struct LargeArrayComponent {
    data: [u8; 1024],
}

#[test]
fn test_large_array_component() {
    let comp = LargeArrayComponent { data: [0u8; 1024] };
    assert!(comp.as_any().is::<LargeArrayComponent>());
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
    assert!(comp_with_some.as_any().is::<OptionComponent>());

    let comp_with_none = OptionComponent { optional: None };
    assert!(comp_with_none.as_any().is::<OptionComponent>());
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

    let any_mut = comp.as_any_mut();
    let downcast = any_mut.downcast_mut::<VecFieldComponent>().unwrap();
    downcast.items.push(4);

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
    let pos = PositionComponent {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let vel = VelocityComponent {
        dx: 0.5,
        dy: 0.5,
        dz: 0.5,
    };
    let name = NameComponent {
        name: "Entity1".to_string(),
    };

    assert!(pos.as_any().is::<PositionComponent>());
    assert!(vel.as_any().is::<VelocityComponent>());
    assert!(name.as_any().is::<NameComponent>());

    // Verify type IDs are distinct
    use std::any::TypeId;
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
    let comp3 = ConstGenericComponent::<3> { data: [1, 2, 3] };
    assert!(comp3.as_any().is::<ConstGenericComponent<3>>());

    let comp5 = ConstGenericComponent::<5> {
        data: [1, 2, 3, 4, 5],
    };
    assert!(comp5.as_any().is::<ConstGenericComponent<5>>());

    // Different const generics create different types
    use std::any::TypeId;
    assert_ne!(
        TypeId::of::<ConstGenericComponent<3>>(),
        TypeId::of::<ConstGenericComponent<5>>()
    );
}
