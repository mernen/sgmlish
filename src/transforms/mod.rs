//! Utility functions and types for transforming [`SgmlFragment`]s.
//!
//! Transforms usually normalize data into a more suitable format for deserialization.
//!
//! [`SgmlFragment`]: crate::SgmlFragment

pub use self::data::*;
pub use self::helper::*;
pub use self::marked_sections::*;
pub use self::normalize_end_tags::*;

mod data;
mod helper;
mod marked_sections;
mod normalize_end_tags;
