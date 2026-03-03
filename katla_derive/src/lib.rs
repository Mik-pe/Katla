//! Derive macros for the Katla engine.
//!
//! This crate provides procedural macros to reduce boilerplate when implementing
//! engine traits.

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
/// Import it with `use katla_ecs::Component;` or have it available in your module.
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
#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics Component for #name #ty_generics #where_clause {
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
