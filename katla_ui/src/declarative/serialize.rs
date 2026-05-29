use katla_math::Color;

use crate::style::FontSize;
use crate::types::TextureId;

use super::descriptor::{
    ChildDescriptor, Padding, StackDescriptor, ViewDescriptor, ZStackDescriptor,
};

/// Serializable subset of ViewDescriptor with no callbacks or fn pointers.
///
/// This is a plain data type suitable for serialization. Users can derive
/// `Serialize`/`Deserialize` on it themselves, or add serde later.
#[derive(Clone, Debug)]
pub enum ViewDescriptorData {
    Text {
        content: String,
        color: Option<[f32; 4]>,
        font_size: Option<String>,
    },
    Button {
        label: String,
        fill_color: Option<[f32; 4]>,
    },
    Progress {
        value: f32,
        range: [f32; 2],
    },
    HStack {
        children: Vec<ViewDescriptorData>,
        spacing: f32,
        padding: [f32; 4],
    },
    VStack {
        children: Vec<ViewDescriptorData>,
        spacing: f32,
        padding: [f32; 4],
    },
    ZStack {
        children: Vec<ViewDescriptorData>,
    },
    Image {
        texture_path: String,
    },
    Empty,
}

/// Resolver that maps string binding keys to actual values.
///
/// Implement this trait to connect data-driven UI descriptors to your
/// application's state (ECS components, script variables, etc.).
pub trait BindingResolver {
    fn resolve_f32(&self, key: &str) -> Option<f32>;
    fn resolve_u32(&self, key: &str) -> Option<u32>;
    fn resolve_string(&self, key: &str) -> Option<String>;
    fn resolve_bool(&self, key: &str) -> Option<bool>;
}

fn color_from_array(arr: [f32; 4]) -> Color {
    Color::new(arr[0], arr[1], arr[2], arr[3])
}

fn parse_font_size(s: &str) -> FontSize {
    match s {
        "XSmall" => FontSize::XSmall,
        "Small" => FontSize::Small,
        "Medium" => FontSize::Medium,
        "Large" => FontSize::Large,
        "XLarge" => FontSize::XLarge,
        _ => FontSize::Medium,
    }
}

fn padding_from_array(arr: [f32; 4]) -> Padding {
    Padding {
        top: arr[0],
        right: arr[1],
        bottom: arr[2],
        left: arr[3],
    }
}

/// Convert ViewDescriptorData to ViewDescriptor using a resolver for bindings.
///
/// The resolver provides values for any data-bound properties. Descriptors
/// that reference bindings the resolver cannot provide will use defaults.
pub fn resolve_descriptor(
    data: &ViewDescriptorData,
    _resolver: &dyn BindingResolver,
    texture_lookup: &dyn Fn(&str) -> Option<TextureId>,
) -> ViewDescriptor {
    match data {
        ViewDescriptorData::Text {
            content,
            color,
            font_size,
        } => ViewDescriptor::Text {
            content: content.clone(),
            color: color.map(color_from_array),
            font_size: font_size.as_deref().map(parse_font_size),
        },

        ViewDescriptorData::Button { label, fill_color } => ViewDescriptor::Button {
            label: label.clone(),
            fill_color: fill_color.map(color_from_array),
            hover_color: None,
            border_color: None,
            on_click: None,
        },

        ViewDescriptorData::Progress { value, range } => ViewDescriptor::Progress {
            value: *value,
            range: range[0]..=range[1],
            fill_color: None,
            label: None,
        },

        ViewDescriptorData::HStack {
            children,
            spacing,
            padding,
        } => ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: children
                .iter()
                .map(|c| ChildDescriptor::from(resolve_descriptor(c, _resolver, texture_lookup)))
                .collect(),
            spacing: *spacing,
            padding: padding_from_array(*padding),
            alignment: super::descriptor::Alignment::Leading,
        })),

        ViewDescriptorData::VStack {
            children,
            spacing,
            padding,
        } => ViewDescriptor::VStack(Box::new(StackDescriptor {
            children: children
                .iter()
                .map(|c| ChildDescriptor::from(resolve_descriptor(c, _resolver, texture_lookup)))
                .collect(),
            spacing: *spacing,
            padding: padding_from_array(*padding),
            alignment: super::descriptor::Alignment::Leading,
        })),

        ViewDescriptorData::ZStack { children } => {
            ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
                children: children
                    .iter()
                    .map(|c| {
                        (
                            super::descriptor::Alignment::Center,
                            ChildDescriptor::from(resolve_descriptor(c, _resolver, texture_lookup)),
                        )
                    })
                    .collect(),
                padding: Padding::zero(),
            }))
        }

        ViewDescriptorData::Image { texture_path } => {
            let texture = texture_lookup(texture_path).unwrap_or(TextureId(0));
            ViewDescriptor::Image {
                texture,
                uv: None,
                tint: Color::WHITE,
            }
        }

        ViewDescriptorData::Empty => ViewDescriptor::Empty,
    }
}
