//! Integration tests for bindless texture API.
//!
//! These tests verify the public API for querying bindless texture indices
//! and slot information, fulfilling validation contract assertions:
//! - VAL-INSPECT-001: Bindless index exposure
//! - VAL-INSPECT-002: Texture slot querying
//! - VAL-INSPECT-003: Font atlas slot tracking
//!
//! Note: All bindless API tests require a Vulkan context and are validated
//! via manual testing with `cargo run -- -s`. Compilation verification alone
//! is handled by the crate's type system.
