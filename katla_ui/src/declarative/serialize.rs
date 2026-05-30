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
            flex: super::descriptor::FlexProps::default(),
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
            flex: super::descriptor::FlexProps::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct NoopResolver;

    impl BindingResolver for NoopResolver {
        fn resolve_f32(&self, _key: &str) -> Option<f32> {
            None
        }
        fn resolve_u32(&self, _key: &str) -> Option<u32> {
            None
        }
        fn resolve_string(&self, _key: &str) -> Option<String> {
            None
        }
        fn resolve_bool(&self, _key: &str) -> Option<bool> {
            None
        }
    }

    struct HashMapResolver {
        f32s: HashMap<String, f32>,
        u32s: HashMap<String, u32>,
        strings: HashMap<String, String>,
        bools: HashMap<String, bool>,
    }

    impl HashMapResolver {
        fn new() -> Self {
            Self {
                f32s: HashMap::new(),
                u32s: HashMap::new(),
                strings: HashMap::new(),
                bools: HashMap::new(),
            }
        }
    }

    impl BindingResolver for HashMapResolver {
        fn resolve_f32(&self, key: &str) -> Option<f32> {
            self.f32s.get(key).copied()
        }
        fn resolve_u32(&self, key: &str) -> Option<u32> {
            self.u32s.get(key).copied()
        }
        fn resolve_string(&self, key: &str) -> Option<String> {
            self.strings.get(key).cloned()
        }
        fn resolve_bool(&self, key: &str) -> Option<bool> {
            self.bools.get(key).copied()
        }
    }

    fn noop_lookup(_path: &str) -> Option<TextureId> {
        None
    }

    #[test]
    fn test_resolve_text_basic() {
        let data = ViewDescriptorData::Text {
            content: "Hello".to_string(),
            color: None,
            font_size: None,
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::Text {
            content,
            color,
            font_size,
        } = result
        else {
            panic!("expected Text");
        };
        assert_eq!(content, "Hello");
        assert!(color.is_none());
        assert!(font_size.is_none());
    }

    #[test]
    fn test_resolve_text_with_color_and_font_size() {
        let data = ViewDescriptorData::Text {
            content: "Colored".to_string(),
            color: Some([1.0, 0.0, 0.0, 1.0]),
            font_size: Some("Large".to_string()),
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::Text {
            content,
            color,
            font_size,
        } = result
        else {
            panic!("expected Text");
        };
        assert_eq!(content, "Colored");
        let c = color.expect("expected Some(color)");
        assert_eq!(c.r, 1.0);
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
        assert_eq!(c.a, 1.0);
        assert_eq!(font_size, Some(FontSize::Large));
    }

    #[test]
    fn test_resolve_button_basic() {
        let data = ViewDescriptorData::Button {
            label: "Click".to_string(),
            fill_color: Some([0.0, 0.0, 1.0, 1.0]),
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::Button {
            label,
            fill_color,
            on_click,
            ..
        } = result
        else {
            panic!("expected Button");
        };
        assert_eq!(label, "Click");
        let c = fill_color.expect("expected Some(fill_color)");
        assert_eq!(c.b, 1.0);
        assert!(on_click.is_none());
    }

    #[test]
    fn test_resolve_progress() {
        let data = ViewDescriptorData::Progress {
            value: 0.5,
            range: [0.0, 1.0],
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::Progress { value, range, .. } = result else {
            panic!("expected Progress");
        };
        assert!((value - 0.5).abs() < f32::EPSILON);
        assert_eq!(range, 0.0..=1.0);
    }

    #[test]
    fn test_resolve_hstack_with_children() {
        let data = ViewDescriptorData::HStack {
            children: vec![
                ViewDescriptorData::Text {
                    content: "A".to_string(),
                    color: None,
                    font_size: None,
                },
                ViewDescriptorData::Text {
                    content: "B".to_string(),
                    color: None,
                    font_size: None,
                },
            ],
            spacing: 8.0,
            padding: [4.0, 4.0, 4.0, 4.0],
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::HStack(desc) = result else {
            panic!("expected HStack");
        };
        assert!((desc.spacing - 8.0).abs() < f32::EPSILON);
        assert_eq!(desc.padding.top, 4.0);
        assert_eq!(desc.padding.right, 4.0);
        assert_eq!(desc.padding.bottom, 4.0);
        assert_eq!(desc.padding.left, 4.0);
        assert_eq!(desc.children.len(), 2);
    }

    #[test]
    fn test_resolve_vstack_with_children() {
        let data = ViewDescriptorData::VStack {
            children: vec![ViewDescriptorData::Text {
                content: "Only".to_string(),
                color: None,
                font_size: None,
            }],
            spacing: 4.0,
            padding: [0.0; 4],
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::VStack(desc) = result else {
            panic!("expected VStack");
        };
        assert!((desc.spacing - 4.0).abs() < f32::EPSILON);
        assert_eq!(desc.padding, Padding::zero());
        assert_eq!(desc.children.len(), 1);
    }

    #[test]
    fn test_resolve_zstack_with_children() {
        let data = ViewDescriptorData::ZStack {
            children: vec![
                ViewDescriptorData::Text {
                    content: "A".to_string(),
                    color: None,
                    font_size: None,
                },
                ViewDescriptorData::Text {
                    content: "B".to_string(),
                    color: None,
                    font_size: None,
                },
            ],
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::ZStack(desc) = result else {
            panic!("expected ZStack");
        };
        assert_eq!(desc.children.len(), 2);
    }

    #[test]
    fn test_resolve_empty() {
        let data = ViewDescriptorData::Empty;
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);
        assert!(matches!(result, ViewDescriptor::Empty));
    }

    #[test]
    fn test_resolve_image_with_texture_lookup() {
        let data = ViewDescriptorData::Image {
            texture_path: "test.png".to_string(),
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &|path| {
            if path == "test.png" {
                Some(TextureId(42))
            } else {
                None
            }
        });

        let ViewDescriptor::Image { texture, .. } = result else {
            panic!("expected Image");
        };
        assert_eq!(texture.0, 42);
    }

    #[test]
    fn test_resolve_image_missing_texture() {
        let data = ViewDescriptorData::Image {
            texture_path: "missing.png".to_string(),
        };
        let resolver = NoopResolver;
        let result = resolve_descriptor(&data, &resolver, &noop_lookup);

        let ViewDescriptor::Image { texture, .. } = result else {
            panic!("expected Image");
        };
        assert_eq!(texture.0, 0);
    }

    #[test]
    fn test_binding_resolver_returns_none_for_missing_keys() {
        let resolver = NoopResolver;
        assert!(resolver.resolve_f32("anything").is_none());
        assert!(resolver.resolve_u32("anything").is_none());
        assert!(resolver.resolve_string("anything").is_none());
        assert!(resolver.resolve_bool("anything").is_none());
    }

    #[test]
    fn test_binding_resolver_returns_values() {
        let mut resolver = HashMapResolver::new();
        resolver.f32s.insert("volume".to_string(), 0.8);
        resolver
            .strings
            .insert("name".to_string(), "test".to_string());

        assert_eq!(resolver.resolve_f32("volume"), Some(0.8));
        assert_eq!(resolver.resolve_string("name"), Some("test".to_string()));
        assert_eq!(resolver.resolve_f32("missing"), None);
    }
}
