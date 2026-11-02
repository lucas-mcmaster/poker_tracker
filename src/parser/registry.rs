//! Runtime selection of the right site parser for each hand history file.

use thiserror::Error;

use crate::parser::base::{HandParser, first_content_line};

/// Why no single parser could be chosen for a file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DetectionError {
    #[error(
        "no registered parser recognizes this file (first line: '{}')",
        first_line.as_deref().unwrap_or("<empty>")
    )]
    Unrecognized { first_line: Option<String> },

    #[error("several registered parsers claim this file: {candidates:?}")]
    Ambiguous { candidates: Vec<String> },
}

/// Ordered collection of site parsers. Each file must be recognized by exactly one parser.
///
/// One parser instance corresponds to one *source*: a site format plus anything the format
/// does not state itself (for GGPoker, the account currency). Registering two parsers that
/// recognize the same format is reported as [`DetectionError::Ambiguous`] rather than guessed.
#[derive(Default)]
pub struct ParserRegistry {
    parsers: Vec<Box<dyn HandParser>>,
}

impl ParserRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a parser (builder style).
    #[must_use]
    pub fn with(mut self, parser: impl HandParser + 'static) -> Self {
        self.register(parser);
        self
    }

    /// Adds a parser.
    pub fn register(&mut self, parser: impl HandParser + 'static) {
        self.parsers.push(Box::new(parser));
    }

    /// Number of registered parsers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parsers.len()
    }

    /// `true` if no parser is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parsers.is_empty()
    }

    /// Picks the single parser that recognizes `raw_session`.
    ///
    /// # Errors
    /// Returns [`DetectionError`] if no parser, or more than one, recognizes the text.
    pub fn detect(&self, raw_session: &str) -> Result<&dyn HandParser, DetectionError> {
        let mut matches = self.parsers.iter().filter(|p| p.recognizes(raw_session));
        match (matches.next(), matches.next()) {
            (Some(only), None) => Ok(only.as_ref()),
            (None, _) => Err(DetectionError::Unrecognized {
                first_line: first_content_line(raw_session).map(|l| l.chars().take(80).collect()),
            }),
            (Some(first), Some(second)) => {
                let mut candidates = vec![first.describe(), second.describe()];
                candidates.extend(matches.map(|p| p.describe()));
                Err(DetectionError::Ambiguous { candidates })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::canonical::{CanonicalHand, Platform};
    use crate::parser::base::ParseError;

    /// Minimal parser that recognizes files whose first line starts with `prefix`.
    struct PrefixParser {
        prefix: &'static str,
    }

    impl HandParser for PrefixParser {
        fn platform(&self) -> Platform {
            Platform::GGPoker
        }
        fn describe(&self) -> String {
            format!("prefix {}", self.prefix)
        }
        fn recognizes(&self, raw_session: &str) -> bool {
            first_content_line(raw_session).is_some_and(|l| l.starts_with(self.prefix))
        }
        fn parse_hand(&self, _raw_block: &str) -> Result<CanonicalHand, ParseError> {
            Err(ParseError::EmptyHandBlock)
        }
        fn peek_hand_id(&self, _raw_block: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn detects_exactly_one_parser_or_explains_why_not() {
        let registry = ParserRegistry::new()
            .with(PrefixParser { prefix: "Alpha" })
            .with(PrefixParser { prefix: "Beta" });
        assert_eq!(registry.len(), 2);

        let chosen = registry.detect("\n\nBeta Hand #1\nbody").unwrap();
        assert_eq!(chosen.describe(), "prefix Beta");

        assert_eq!(
            registry.detect("Gamma Hand #1").err(),
            Some(DetectionError::Unrecognized {
                first_line: Some("Gamma Hand #1".into())
            })
        );

        let ambiguous = ParserRegistry::new()
            .with(PrefixParser { prefix: "Al" })
            .with(PrefixParser { prefix: "Alpha" });
        assert!(matches!(
            ambiguous.detect("Alpha Hand #1"),
            Err(DetectionError::Ambiguous { candidates }) if candidates.len() == 2
        ));
        assert!(ParserRegistry::new().is_empty());
    }

    #[test]
    fn parse_session_uses_the_parsers_block_splitter() {
        let p = PrefixParser { prefix: "Alpha" };
        let results: Vec<_> = p.parse_session("Alpha 1\n\nAlpha 2\n").collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[1].0.start_line, 3);
        assert!(results.iter().all(|(_, r)| r.is_err()));
    }
}
