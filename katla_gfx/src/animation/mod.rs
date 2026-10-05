pub mod types;

pub use types::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};

mod upload;
pub use upload::{AnimationBufferUploader, AnimationUpload};
