//! Derive macros for the Katla engine.
//!
//! This crate provides procedural macros to reduce boilerplate when implementing
//! engine traits.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Meta, Expr, Lit};

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

/// Derive macro for the Material trait.
///
/// This macro automatically implements the `Material` trait for your struct
/// using field values and helper attributes.
///
/// # Required Fields
///
/// The struct must have these fields (or use attributes to override):
/// - `vertex_binding: VertexBinding` - Vertex format description
/// - `descriptor_layouts: Vec<DescriptorSetLayoutBuilder>` - Descriptor layouts
/// - `shader_path: PathBuf` - Shader path (if no shader attribute)
///
/// # Attributes
///
/// The following helper attributes are supported on the struct:
///
/// - `#[material(shader = "path/to/shader.wgsl")]` - Path to both vertex and fragment shader
/// - `#[material(vertex_shader = "path")]` - Path to vertex shader only
/// - `#[material(fragment_shader = "path")]` - Path to fragment shader only
/// - `#[material(domain = "Surface")]` - Material domain (Surface, Ui, PostProcess, Particle)
/// - `#[material(depth_test = true)]` - Enable/disable depth test
/// - `#[material(depth_write = true)]` - Enable/disable depth write
/// - `#[material(cull_backfaces = true)]` - Enable/disable backface culling
/// - `#[material(alpha_blending = false)]` - Enable/disable alpha blending
/// - `#[material(color_format = "R16G16B16A16Sfloat")]` - Color attachment format
/// - `#[material(depth_format = "D32SfloatS8Uint")]` - Depth attachment format
/// - `#[material(uses_pbr = true)]` - Material uses PBR textures (5 textures)
/// - `#[material(uses_skeleton = true)]` - Material uses skeleton for animation
/// - `#[material(uses_bindless = true)]` - Material uses bindless textures
///
/// # Example
///
/// ```ignore
/// use katla_vulkan::{
///     Material, VertexBinding, ShaderSource, RenderState,
///     DescriptorSetLayoutBuilder, DescriptorType, ShaderStages,
///     MaterialDomain, ImageFormat,
/// };
/// use katla_derive::Material;
/// use std::path::PathBuf;
///
/// #[derive(Material)]
/// #[material(shader = "resources/shaders/sky.wgsl")]
/// #[material(domain = "PostProcess")]
/// #[material(depth_test = true, depth_write = false)]
/// pub struct SkyMaterialConfig {
///     pub vertex_binding: VertexBinding,
///     pub shader_path: PathBuf,
///     pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
/// }
/// ```
#[proc_macro_derive(Material, attributes(material))]
pub fn derive_material(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Parse material attributes
    let mut shader: Option<String> = None;
    let mut vertex_shader: Option<String> = None;
    let mut fragment_shader: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut depth_test: Option<bool> = None;
    let mut depth_write: Option<bool> = None;
    let mut cull_backfaces: Option<bool> = None;
    let mut alpha_blending: Option<bool> = None;
    let mut color_format: Option<String> = None;
    let mut depth_format: Option<String> = None;
    let mut uses_pbr: Option<bool> = None;
    let mut uses_skeleton: Option<bool> = None;
    let mut uses_bindless: Option<bool> = None;

    for attr in &input.attrs {
        if attr.path().is_ident("material") {
            if let Ok(meta) = attr.parse_args::<Meta>() {
                match meta {
                    Meta::NameValue(nv) => {
                        if let Some(ident) = nv.path.get_ident() {
                            let key = ident.to_string();
                            if let Expr::Lit(expr_lit) = &nv.value {
                                if let Lit::Str(lit_str) = &expr_lit.lit {
                                    match key.as_str() {
                                        "shader" => shader = Some(lit_str.value()),
                                        "vertex_shader" => vertex_shader = Some(lit_str.value()),
                                        "fragment_shader" => fragment_shader = Some(lit_str.value()),
                                        "domain" => domain = Some(lit_str.value()),
                                        "color_format" => color_format = Some(lit_str.value()),
                                        "depth_format" => depth_format = Some(lit_str.value()),
                                        _ => {}
                                    }
                                } else if let Lit::Bool(lit_bool) = &expr_lit.lit {
                                    match key.as_str() {
                                        "depth_test" => depth_test = Some(lit_bool.value()),
                                        "depth_write" => depth_write = Some(lit_bool.value()),
                                        "cull_backfaces" => cull_backfaces = Some(lit_bool.value()),
                                        "alpha_blending" => alpha_blending = Some(lit_bool.value()),
                                        "uses_pbr" => uses_pbr = Some(lit_bool.value()),
                                        "uses_skeleton" => uses_skeleton = Some(lit_bool.value()),
                                        "uses_bindless" => uses_bindless = Some(lit_bool.value()),
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // shader attribute sets both vertex and fragment
    if let Some(ref s) = shader {
        if vertex_shader.is_none() {
            vertex_shader = Some(s.clone());
        }
        if fragment_shader.is_none() {
            fragment_shader = Some(s.clone());
        }
    }

    // Generate shader source expressions
    let vertex_shader_expr = match vertex_shader {
        Some(path) => quote! {
            katla_vulkan::material::ShaderSource::WgslFile(::std::path::PathBuf::from(#path))
        },
        None => quote! {
            katla_vulkan::material::ShaderSource::WgslFile(self.shader_path.clone())
        },
    };

    let fragment_shader_expr = match fragment_shader {
        Some(path) => quote! {
            katla_vulkan::material::ShaderSource::WgslFile(::std::path::PathBuf::from(#path))
        },
        None => quote! {
            katla_vulkan::material::ShaderSource::WgslFile(self.shader_path.clone())
        },
    };

    // Generate domain expression (only 4 valid domains now)
    let domain_expr = match domain.as_deref() {
        Some("Surface") => quote! { katla_vulkan::MaterialDomain::Surface },
        Some("Ui") => quote! { katla_vulkan::MaterialDomain::Ui },
        Some("PostProcess") => quote! { katla_vulkan::MaterialDomain::PostProcess },
        Some("Particle") => quote! { katla_vulkan::MaterialDomain::Particle },
        _ => quote! { katla_vulkan::MaterialDomain::Surface },
    };

    // Generate render state
    let render_state_code = {
        let depth_test_code = match depth_test {
            Some(v) => quote! { depth_test: #v, },
            None => quote! { depth_test: true, },
        };
        let depth_write_code = match depth_write {
            Some(v) => quote! { depth_write: #v, },
            None => quote! { depth_write: true, },
        };
        let cull_code = match cull_backfaces {
            Some(v) => quote! { cull_backfaces: #v, },
            None => quote! { cull_backfaces: true, },
        };
        let alpha_code = match alpha_blending {
            Some(v) => quote! { alpha_blending: #v, },
            None => quote! { alpha_blending: false, },
        };

        quote! {
            katla_vulkan::material::RenderState {
                #depth_test_code
                #depth_write_code
                #cull_code
                #alpha_code
            }
        }
    };

    // Generate color format expression
    let color_format_expr = match color_format.as_deref() {
        Some("B8G8R8A8Srgb") => quote! { katla_vulkan::ImageFormat::B8G8R8A8Srgb },
        Some("R8G8B8A8Srgb") => quote! { katla_vulkan::ImageFormat::R8G8B8A8Srgb },
        Some("R16G16B16A16Sfloat") => quote! { katla_vulkan::ImageFormat::R16G16B16A16Sfloat },
        _ => quote! { katla_vulkan::ImageFormat::R16G16B16A16Sfloat },
    };

    // Generate depth format expression
    let depth_format_expr = match depth_format.as_deref() {
        Some("D32Sfloat") => quote! { katla_vulkan::ImageFormat::D32Sfloat },
        Some("D32SfloatS8Uint") => quote! { katla_vulkan::ImageFormat::D32SfloatS8Uint },
        Some("D24UnormS8Uint") => quote! { katla_vulkan::ImageFormat::D24UnormS8Uint },
        _ => quote! { katla_vulkan::ImageFormat::D32SfloatS8Uint },
    };

    // Generate uses_pbr_textures method
    let uses_pbr_expr = match uses_pbr {
        Some(v) => quote! { #v },
        None => quote! { false },
    };

    // Generate uses_skeleton method
    let uses_skeleton_expr = match uses_skeleton {
        Some(v) => quote! { #v },
        None => quote! { false },
    };

    // Generate uses_bindless method
    let uses_bindless_expr = match uses_bindless {
        Some(v) => quote! { #v },
        None => quote! { false },
    };

    // Generate the implementation
    // Note: Uses katla_vulkan:: prefixed types so users don't need to import them
    let expanded = quote! {
        impl #impl_generics katla_vulkan::Material for #name #ty_generics #where_clause {
            fn vertex_shader(&self) -> katla_vulkan::material::ShaderSource {
                #vertex_shader_expr
            }

            fn fragment_shader(&self) -> katla_vulkan::material::ShaderSource {
                #fragment_shader_expr
            }

            fn vertex_binding(&self) -> katla_vulkan::VertexBinding {
                self.vertex_binding.clone()
            }

            fn render_state(&self) -> katla_vulkan::material::RenderState {
                #render_state_code
            }

            fn descriptor_layouts(&self) -> ::std::vec::Vec<katla_vulkan::DescriptorSetLayoutBuilder> {
                self.descriptor_layouts.clone()
            }

            fn domain(&self) -> katla_vulkan::MaterialDomain {
                #domain_expr
            }

            fn color_format(&self) -> katla_vulkan::ImageFormat {
                #color_format_expr
            }

            fn depth_format(&self) -> katla_vulkan::ImageFormat {
                #depth_format_expr
            }

            fn uses_pbr_textures(&self) -> bool {
                #uses_pbr_expr
            }

            fn uses_skeleton(&self) -> bool {
                #uses_skeleton_expr
            }

            fn uses_bindless(&self) -> bool {
                #uses_bindless_expr
            }
        }
    };

    TokenStream::from(expanded)
}
