use katla_math::Color;

use crate::style::FontSize;

use super::descriptor::Padding;

/// Serializable subset of a widget tree with no callbacks or fn pointers.
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

fn _color_from_array(arr: [f32; 4]) -> Color {
    Color::new(arr[0], arr[1], arr[2], arr[3])
}

fn _parse_font_size(s: &str) -> FontSize {
    match s {
        "XSmall" => FontSize::XSmall,
        "Small" => FontSize::Small,
        "Medium" => FontSize::Medium,
        "Large" => FontSize::Large,
        "XLarge" => FontSize::XLarge,
        _ => FontSize::Medium,
    }
}

fn _padding_from_array(arr: [f32; 4]) -> Padding {
    Padding {
        top: arr[0],
        right: arr[1],
        bottom: arr[2],
        left: arr[3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_binding_resolver_returns_none_for_missing_keys() {
        let resolver = NoopResolver;
        assert!(resolver.resolve_f32("anything").is_none());
        assert!(resolver.resolve_u32("anything").is_none());
        assert!(resolver.resolve_string("anything").is_none());
        assert!(resolver.resolve_bool("anything").is_none());
    }
}
