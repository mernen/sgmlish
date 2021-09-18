//! Items related to parsing marked sections.

use std::str::FromStr;

const KEYWORDS: &[(&str, MarkedSectionStatus)] = &[
    ("CDATA", MarkedSectionStatus::CData),
    ("RCDATA", MarkedSectionStatus::RcData),
    ("IGNORE", MarkedSectionStatus::Ignore),
    ("INCLUDE", MarkedSectionStatus::Include),
    ("TEMP", MarkedSectionStatus::Include),
];

/// The different levels a marked section may have, depending on its keywords.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MarkedSectionStatus {
    Include,
    RcData,
    CData,
    Ignore,
}

impl MarkedSectionStatus {
    /// Returns the highest-priority operation from all the given keywords.
    ///
    /// When no keywords are present, the default status is [`Include`](MarkedSectionStatus::Include).
    /// If the keyword list contains an invalid keyword, returns it as an error.
    pub fn from_keywords(status_keywords: &str) -> Result<Self, &str> {
        status_keywords
            .split_ascii_whitespace()
            .map(|keyword| keyword.parse().map_err(|_| keyword))
            .try_fold(MarkedSectionStatus::Include, |a, b| b.map(|b| a.max(b)))
    }
}

impl Default for MarkedSectionStatus {
    /// Returns the default status for marked sections: [`Include`](MarkedSectionStatus::Include).
    fn default() -> Self {
        MarkedSectionStatus::Include
    }
}

impl FromStr for MarkedSectionStatus {
    type Err = ParseMarkedSectionStatusError;

    /// Parses a single status keyword from a string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        KEYWORDS
            .iter()
            .find_map(|(kw, level)| kw.eq_ignore_ascii_case(s).then(|| *level))
            .ok_or(ParseMarkedSectionStatusError)
    }
}

/// When a marked section status keyword is not one of `CDATA`, `RCDATA`, `IGNORE`, `INCLUDE`, or `TEMP`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ParseMarkedSectionStatusError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marked_section_status_priority() {
        assert!(MarkedSectionStatus::Ignore > MarkedSectionStatus::CData);
        assert!(MarkedSectionStatus::CData > MarkedSectionStatus::RcData);
        assert!(MarkedSectionStatus::RcData > MarkedSectionStatus::Include);
    }

    #[test]
    fn test_marked_section_status_from_str() {
        assert_eq!(
            "igNore".parse::<MarkedSectionStatus>(),
            Ok(MarkedSectionStatus::Ignore)
        );
        assert_eq!(
            "cdaTA".parse::<MarkedSectionStatus>(),
            Ok(MarkedSectionStatus::CData)
        );
        assert_eq!(
            "RcdaTa".parse::<MarkedSectionStatus>(),
            Ok(MarkedSectionStatus::RcData)
        );
        assert_eq!(
            "IncludE".parse::<MarkedSectionStatus>(),
            Ok(MarkedSectionStatus::Include)
        );
        assert_eq!(
            "TEmp".parse::<MarkedSectionStatus>(),
            Ok(MarkedSectionStatus::Include)
        );
        assert_eq!(
            "IGNORED".parse::<MarkedSectionStatus>(),
            Err(ParseMarkedSectionStatusError)
        );
    }

    #[test]
    fn test_marked_section_status_from_keywords() {
        assert_eq!(
            MarkedSectionStatus::from_keywords("ignore cdata"),
            Ok(MarkedSectionStatus::Ignore)
        );
        assert_eq!(
            MarkedSectionStatus::from_keywords("temp include ignore"),
            Ok(MarkedSectionStatus::Ignore)
        );
        assert_eq!(
            MarkedSectionStatus::from_keywords("RCDATA cdata"),
            Ok(MarkedSectionStatus::CData)
        );
        assert_eq!(
            MarkedSectionStatus::from_keywords("ignore unknown temp"),
            Err("unknown")
        );
    }
}
