//! Utility functions and types for transforming [`SgmlFragment`]s.
//!
//! Transforms usually normalize data into a more suitable format for deserialization.
//!
//! [`SgmlFragment`]: crate::SgmlFragment

pub use self::normalize_end_tags::*;
pub use self::transform::*;

mod normalize_end_tags;
mod transform;
