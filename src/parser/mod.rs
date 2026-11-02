//! Site-independent parser contract, parser registry, and site-specific parsers.
//!
//! Layout: `base` (trait, validator, splitter) and `registry` are shared by every site.
//! Each site lives in its own module (`ggpoker`, ...) and is the only place that knows
//! that site's text format.

pub mod base;
pub mod ggpoker;
pub mod registry;

pub use base::{
    HandBlock, HandParser, ParseError, ParsedBlocks, first_content_line, split_hand_blocks,
    validate_hand,
};
pub use ggpoker::GgPokerParser;
pub use registry::{DetectionError, ParserRegistry};
