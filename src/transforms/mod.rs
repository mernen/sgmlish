//! Utility functions and types for transforming [`SgmlFragment`]s.
//!
//! Transforms usually normalize data into a more suitable format for deserialization.
//!
//! [`SgmlFragment`]: crate::SgmlFragment

pub use self::helper::*;
pub use self::normalize_end_tags::*;

mod helper;
mod normalize_end_tags;
