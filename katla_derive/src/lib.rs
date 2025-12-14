//! Derive macros for the Katla ECS framework.
//!
//! This crate provides procedural macros to reduce boilerplate when implementing
//! ECS traits.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive macro for the Component trait.
///
/// This macro automatically implements the `Component` trait for your struct,
/// providing the required `as_any` and `as_any_mut` methods.
///
/// # Requirements
///
/// The `Component` trait must be in scope when using this derive macro.
/// Import it with `use katla::ecs::Component;` or have it available in your module.
///
/// # Example
///
/// ```ignore
/// use katla_ecs::Component;
///
/// #[derive(Component)]
/// struct HealthComponent {
///     current: f32,
///     max: f32,
/// }
/// ```
///
/// This will expand to:
///
/// ```ignore
/// impl Component for HealthComponent {
///     fn as_any(&self) -> &dyn std::any::Any {
///         self
///     }
///
///     fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
///         self
///     }
/// }
/// ```
#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Generate the implementation
    // Use unqualified Component which will be resolved from scope
    // This allows it to work both inside the katla crate and in user code
    let expanded = quote! {
        impl Component for #name {
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }
        }
    };

    TokenStream::from(expanded)
}
