//! Derive macros for the Katla engine.
//!
//! This crate provides procedural macros to reduce boilerplate when implementing
//! engine traits.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Field, Fields, Lit, Type, TypePath};

#[derive(Default)]
struct InspectAttr {
    skip: bool,
    color: bool,
    min: Option<f32>,
    max: Option<f32>,
    speed: Option<f32>,
    display_name: Option<String>,
}

fn parse_inspect_attr(field: &Field) -> InspectAttr {
    let mut attr = InspectAttr::default();

    for meta_item in field.attrs.iter().filter(|a| a.path().is_ident("inspect")) {
        let _ = meta_item.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                attr.skip = true;
            } else if meta.path.is_ident("color") {
                attr.color = true;
            } else if meta.path.is_ident("range") {
                let content;
                syn::parenthesized!(content in meta.input);
                let min_val: Lit = content.parse()?;
                let _: syn::Token![,] = content.parse()?;
                let max_val: Lit = content.parse()?;
                let min: f32 = match min_val {
                    Lit::Float(f) => f.base10_parse::<f32>().unwrap(),
                    Lit::Int(i) => i.base10_parse::<f32>().unwrap(),
                    _ => panic!("range() expects numeric literals"),
                };
                let max: f32 = match max_val {
                    Lit::Float(f) => f.base10_parse::<f32>().unwrap(),
                    Lit::Int(i) => i.base10_parse::<f32>().unwrap(),
                    _ => panic!("range() expects numeric literals"),
                };
                attr.min = Some(min);
                attr.max = Some(max);
            } else if meta.path.is_ident("speed") {
                let val = meta.value()?;
                let lit: Lit = val.parse()?;
                match lit {
                    Lit::Float(f) => attr.speed = Some(f.base10_parse::<f32>().unwrap()),
                    Lit::Int(i) => attr.speed = Some(i.base10_parse::<f32>().unwrap()),
                    _ => panic!("speed() expects a numeric literal"),
                }
            } else if meta.path.is_ident("display_name") {
                let val = meta.value()?;
                let lit: Lit = val.parse()?;
                match lit {
                    Lit::Str(s) => attr.display_name = Some(s.value()),
                    _ => panic!("display_name expects a string literal"),
                }
            }
            Ok(())
        });
    }

    attr
}

fn make_display_name(field_name: &str) -> String {
    field_name
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn infer_field_kind(ty: &Type, is_color: bool) -> (proc_macro2::TokenStream, bool) {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    if is_color {
        return (quote! { ::katla_ecs::inspect::FieldKind::Color }, false);
    }

    // Check if the type path contains "color" (case-insensitive)
    if let Some(seg) = get_last_type_segment(ty) {
        let name = seg.ident.to_string().to_lowercase();
        if name.contains("color") || name.contains("colour") {
            return (quote! { ::katla_ecs::inspect::FieldKind::Color }, false);
        }
    }

    match type_str.as_str() {
        "f32" => (quote! { ::katla_ecs::inspect::FieldKind::Float }, true),
        "f64" => (quote! { ::katla_ecs::inspect::FieldKind::Float }, true),
        "i32" => (quote! { ::katla_ecs::inspect::FieldKind::Int }, true),
        "i64" => (quote! { ::katla_ecs::inspect::FieldKind::Int }, true),
        "u32" => (quote! { ::katla_ecs::inspect::FieldKind::Int }, true),
        "u64" => (quote! { ::katla_ecs::inspect::FieldKind::Int }, true),
        "bool" => (quote! { ::katla_ecs::inspect::FieldKind::Bool }, true),
        "String" => (quote! { ::katla_ecs::inspect::FieldKind::String }, true),
        _ => (quote! { ::katla_ecs::inspect::FieldKind::Unknown }, false),
    }
}

fn get_last_type_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(TypePath { path, .. }) => path.segments.last(),
        _ => None,
    }
}

fn field_mut_arm(field_name: &str, ty: &Type) -> proc_macro2::TokenStream {
    let ident = syn::Ident::new(field_name, proc_macro2::Span::call_site());
    let type_str = quote!(#ty).to_string().replace(' ', "");

    match type_str.as_str() {
        "f32" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::F32(&mut self.#ident))
        },
        "f64" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::F64(&mut self.#ident))
        },
        "i32" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::I32(&mut self.#ident))
        },
        "u32" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::U32(&mut self.#ident))
        },
        "bool" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::Bool(&mut self.#ident))
        },
        "String" => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::String(&mut self.#ident))
        },
        _ => quote! {
            #field_name => Some(::katla_ecs::inspect::FieldMut::Unknown(&mut self.#ident as &mut dyn std::any::Any))
        },
    }
}

/// Derive macro for the Component trait.
///
/// This macro automatically implements the `Component` trait for your struct,
/// and when the `editor` feature is enabled on `katla_ecs`, also implements
/// the `Inspect` trait for runtime reflection.
///
/// # Helper attributes
///
/// Fields can be annotated with `#[inspect(...)]`:
/// - `skip` — exclude from the inspector
/// - `color` — treat as a color value
/// - `range(min, max)` — set numeric range constraints
/// - `speed(f32)` — set editor drag speed
/// - `display_name = "Custom Name"` — override display name
///
/// # Example
///
/// ```ignore
/// use katla_ecs::Component;
///
/// #[derive(Component)]
/// struct HealthComponent {
///     #[inspect(range(0.0, 100.0))]
///     current: f32,
///     max: f32,
/// }
/// ```
#[proc_macro_derive(Component, attributes(inspect))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let component_impl = quote! {
        impl #impl_generics ::katla_ecs::Component for #name #ty_generics #where_clause {}
    };

    let inspect_impl = generate_inspect_impl(&input);

    let expanded = quote! {
        #component_impl

        #inspect_impl
    };

    TokenStream::from(expanded)
}

fn generate_inspect_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => return quote! {},
        },
        _ => return quote! {},
    };

    let mut field_info_entries = Vec::new();
    let mut field_mut_arms = Vec::new();

    for field in fields {
        let field_name = match field.ident.as_ref() {
            Some(ident) => ident.to_string(),
            None => continue,
        };

        let attr = parse_inspect_attr(field);

        if attr.skip {
            continue;
        }

        let ty = &field.ty;
        let type_str = quote!(#ty).to_string();

        let display_name = attr
            .display_name
            .unwrap_or_else(|| make_display_name(&field_name));

        let (kind, _is_primitive) = infer_field_kind(ty, attr.color);

        let min_tokens = match attr.min {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };

        let max_tokens = match attr.max {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };

        let speed_tokens = match attr.speed {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };

        field_info_entries.push(quote! {
            ::katla_ecs::inspect::FieldInfo {
                name: #field_name,
                display_name: #display_name,
                type_name: #type_str,
                kind: #kind,
                constraints: ::katla_ecs::inspect::FieldConstraints {
                    min: #min_tokens,
                    max: #max_tokens,
                    speed: #speed_tokens,
                    skip: false,
                },
            }
        });

        field_mut_arms.push(field_mut_arm(&field_name, ty));
    }

    let field_mut_body = if field_mut_arms.is_empty() {
        quote! {
            match name {
                _ => None,
            }
        }
    } else {
        quote! {
            match name {
                #(#field_mut_arms),*
                ,
                _ => None,
            }
        }
    };

    quote! {
        #[cfg(feature = "editor")]
        impl #impl_generics ::katla_ecs::inspect::Inspect for #name #ty_generics #where_clause {
            fn fields() -> Vec<::katla_ecs::inspect::FieldInfo> {
                vec![
                    #(#field_info_entries),*
                ]
            }

            fn field_mut(&mut self, name: &str) -> Option<::katla_ecs::inspect::FieldMut<'_>> {
                #field_mut_body
            }
        }
    }
}
