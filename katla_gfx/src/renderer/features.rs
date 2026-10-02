//! Optional device services independent of application rendering policy.

/// Optional operation supported by a device implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererFeature {
    /// GPU timestamp collection without blocking submission retirement.
    TimestampQueries,
    /// Replacement of the contents of an existing texture allocation.
    TextureInPlaceUpdate,
    /// Explicit mip, array-layer and three-dimensional texture-region uploads.
    TextureSubresourceUpload,
}

impl RendererFeature {
    /// Every device feature, in stable diagnostic order.
    pub const ALL: &'static [Self] = &[
        Self::TimestampQueries,
        Self::TextureInPlaceUpdate,
        Self::TextureSubresourceUpload,
    ];

    /// Stable machine-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::TimestampQueries => "timestamp_queries",
            Self::TextureInPlaceUpdate => "texture_in_place_update",
            Self::TextureSubresourceUpload => "texture_subresource_upload",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_all_features_have_unique_names() {
        let names: HashSet<_> = RendererFeature::ALL
            .iter()
            .map(|feature| feature.name())
            .collect();
        assert_eq!(names.len(), RendererFeature::ALL.len());
        assert!(names.iter().all(|name| !name.is_empty()));
    }
}
