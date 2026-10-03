//! Read imported image identity and current native fallback status for authoring.

use katla_agent::material_sampling::TextureRole;
use katla_ecs::EntityId;
use serde_json::{Value, json};

pub(super) fn inspect(app: &mut super::Application, entity: EntityId) -> Result<Value, String> {
    use crate::scene::{AssetRef, EntitySource};
    use katla_gfx::{GpuRenderer, TextureHandle};
    let drawable = super::material::drawable(&app.world, entity)?;
    let material = drawable.material_handle;
    let emission = drawable.emission;
    let tangent_basis = if let Some(original) = drawable.tangent_uv {
        json!({"kind":if original==drawable.sampling.normal.uv { "generated_mikktspace" } else { "reconstructed_from_current_uv" },"generated_uv":original})
    } else {
        json!({"kind":"provided"})
    };
    let Some(EntitySource::GltfPrimitive {
        path,
        node_index,
        primitive_index,
    }) = app.world.get_component::<EntitySource>(entity).cloned()
    else {
        return Ok(
            json!({"imported_textures":null,"tangent_basis":tangent_basis,"texture_assignment_editable":false}),
        );
    };
    let path = match path {
        AssetRef::File(path) => path,
        AssetRef::Resource(path) => app.resources.root.join(path),
        AssetRef::Scene(_) => {
            return Err("Runtime glTF origin must be resolved before inspection".into());
        }
    };
    let model = app
        .gltf_cache
        .read(path.clone())
        .map_err(|error| format!("Cannot inspect the loaded glTF origin: {error}"))?;
    let primitive = model
        .primitives
        .iter()
        .find(|primitive| {
            primitive.node_index == node_index && primitive.primitive_index == primitive_index
        })
        .ok_or("Loaded glTF primitive identity is unavailable")?;
    let textures = app.renderer.material_textures(material);
    let handles = textures.map_or([TextureHandle::NONE; 4], |textures| {
        [
            textures.albedo,
            textures.normal,
            textures.metallic_roughness,
            textures.occlusion,
        ]
    });
    let handles = [handles[0], handles[1], handles[2], handles[3], emission];
    let asset = path.strip_prefix(&app.resources.root).map_or_else(
        |_| AssetRef::File(path.clone()),
        |relative| AssetRef::Resource(relative.to_string_lossy().into_owned()),
    );
    let mut entries = serde_json::Map::new();
    for (role, info) in TextureRole::ALL.into_iter().zip([
        primitive.material.base_color_texture,
        primitive.material.normal_texture,
        primitive.material.metallic_roughness_texture,
        primitive.material.occlusion_texture,
        primitive.material.emission_texture,
    ]) {
        let Some(info) = info else {
            entries.insert(
                role.name().into(),
                json!({"source":null,"using_fallback":true}),
            );
            continue;
        };
        let image = model
            .images
            .get(info.image_index)
            .ok_or("Loaded glTF texture image is unavailable")?;
        let color_role = matches!(role, TextureRole::Albedo | TextureRole::Emission);
        let floating = matches!(
            image.format,
            gltf::image::Format::R32G32B32FLOAT | gltf::image::Format::R32G32B32A32FLOAT
        );
        let srgb = color_role && !floating;
        let handle = handles[role.index()];
        let using_fallback = app.gltf_texture_cache.get(&path, info.image_index, srgb)
            != Some(handle)
            || app.renderer.get_bindless_slot(handle).is_none();
        let image_source =
            model
                .document
                .images()
                .nth(info.image_index)
                .map(|image| match image.source() {
                    gltf::image::Source::View { .. } => "embedded_buffer_view".into(),
                    gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => {
                        "embedded_data_uri".into()
                    }
                    gltf::image::Source::Uri { uri, .. } => uri.to_owned(),
                });
        entries.insert(role.name().into(),json!({"source":{"kind":"gltf_image","asset":asset,"image_index":info.image_index,"image_source":image_source},"width":image.width,"height":image.height,"mip_levels":32-image.width.max(image.height).leading_zeros(),"decoded_format":format!("{:?}",image.format),"source_color_space":if srgb { "srgb" } else { "linear" },"sampled_color_space":"linear","using_fallback":using_fallback}));
    }
    Ok(
        json!({"imported_textures":entries,"tangent_basis":tangent_basis,"texture_assignment_editable":false}),
    )
}
