//! Policy-fact extractors.
//!
//! Each extractor module handles a specific policy-fact family
//! for a specific language.
//!
//! PF-1: status_mapping (C only)
//! PF-2: behavioral_marker (C only)
//! PF-3: return_fate (C only)

pub mod behavioral_marker;
pub mod return_fate;
pub mod status_mapping;
