use std::fmt;

/// One piece of a parsed route pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
    /// A static segment suffixed with '?'. Only valid as part of a
    /// trailing run of optional segments.
    OptionalStatic(String),
    /// A param segment suffixed with '?'. Only valid as part of a
    /// trailing run of optional segments.
    OptionalParam(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    pub message: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Splits a pattern like "/users/:id/posts/*rest" into segments.
/// A leading slash is optional; a trailing or doubled slash is an error
/// because it would silently swallow an empty path component.
///
/// A segment suffixed with '?' (e.g. ":format?" or "json?") is optional:
/// the path may end before it. Optional segments must form a run at the
/// end of the pattern, since matching them against the middle of a path
/// would need backtracking the rest of this module doesn't do. A
/// wildcard already matches a variable number of components, so it
/// cannot itself be marked optional.
pub fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, PatternError> {
    let trimmed = pattern.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    if body.is_empty() {
        return Ok(Vec::new());
    }

    let raw_segments: Vec<&str> = body.split('/').collect();
    let mut segments = Vec::with_capacity(raw_segments.len());

    for (i, raw_with_suffix) in raw_segments.iter().enumerate() {
        if raw_with_suffix.is_empty() {
            return Err(PatternError {
                message: format!("empty segment in pattern '{}'", pattern),
            });
        }
        let is_last = i == raw_segments.len() - 1;
        let (raw, optional) = match raw_with_suffix.strip_suffix('?') {
            Some(rest) => (rest, true),
            None => (raw_with_suffix, false),
        };
        if raw.is_empty() {
            return Err(PatternError {
                message: format!("empty segment in pattern '{}'", pattern),
            });
        }

        if let Some(name) = raw.strip_prefix(':') {
            if name.is_empty() {
                return Err(PatternError {
                    message: format!("param segment missing a name in '{}'", pattern),
                });
            }
            segments.push(if optional {
                Segment::OptionalParam(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else if let Some(name) = raw.strip_prefix('*') {
            if name.is_empty() {
                return Err(PatternError {
                    message: format!("wildcard segment missing a name in '{}'", pattern),
                });
            }
            if optional {
                return Err(PatternError {
                    message: format!("wildcard segment cannot be optional in '{}'", pattern),
                });
            }
            if !is_last {
                return Err(PatternError {
                    message: format!("wildcard segment must be last in '{}'", pattern),
                });
            }
            segments.push(Segment::Wildcard(name.to_string()));
        } else {
            segments.push(if optional {
                Segment::OptionalStatic(raw.to_string())
            } else {
                Segment::Static(raw.to_string())
            });
        }
    }

    let mut seen_optional = false;
    for segment in &segments {
        match segment {
            Segment::OptionalStatic(_) | Segment::OptionalParam(_) => seen_optional = true,
            _ if seen_optional => {
                return Err(PatternError {
                    message: format!(
                        "optional segments must be at the end of pattern '{}'",
                        pattern
                    ),
                });
            }
            _ => {}
        }
    }

    Ok(segments)
}

/// Splits a request path into its components. A bare "/" yields no
/// components at all, matching a pattern with no segments.
pub fn split_path(path: &str) -> Vec<&str> {
    let trimmed = path.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    if body.is_empty() {
        Vec::new()
    } else {
        body.split('/').collect()
    }
}

/// Matches parsed segments against a path's components, returning the
/// captured params in declaration order on success.
///
/// A wildcard must capture at least one component; this is a design
/// choice so that "/files/*rest" does not also match bare "/files".
///
/// Trailing optional segments are matched greedily against whatever
/// path components remain once the required segments are consumed: the
/// path may run out at any point in that trailing run, but it cannot
/// skip an earlier optional segment and still match a later one.
pub fn match_segments(
    segments: &[Segment],
    path_segments: &[&str],
) -> Option<Vec<(String, String)>> {
    let optional_start = segments
        .iter()
        .position(|s| matches!(s, Segment::OptionalStatic(_) | Segment::OptionalParam(_)))
        .unwrap_or(segments.len());
    let (required, optional) = segments.split_at(optional_start);

    let mut params = Vec::new();
    let mut path_iter = path_segments.iter();

    for (i, segment) in required.iter().enumerate() {
        match segment {
            Segment::Wildcard(name) => {
                let rest: Vec<&str> = path_iter.by_ref().collect();
                if rest.is_empty() {
                    return None;
                }
                params.push((name.clone(), rest.join("/")));
                return if i == required.len() - 1 {
                    Some(params)
                } else {
                    None
                };
            }
            Segment::Param(name) => {
                let value = path_iter.next()?;
                params.push((name.clone(), value.to_string()));
            }
            Segment::Static(expected) => {
                let value = path_iter.next()?;
                if value != expected {
                    return None;
                }
            }
            Segment::OptionalStatic(_) | Segment::OptionalParam(_) => unreachable!(
                "optional segments are always a trailing run split off into `optional`"
            ),
        }
    }

    for segment in optional {
        let value = match path_iter.next() {
            Some(value) => value,
            None => break,
        };
        match segment {
            Segment::OptionalParam(name) => params.push((name.clone(), value.to_string())),
            Segment::OptionalStatic(expected) => {
                if value != expected {
                    return None;
                }
            }
            _ => unreachable!("`optional` only ever holds OptionalStatic/OptionalParam"),
        }
    }

    if path_iter.next().is_some() {
        None
    } else {
        Some(params)
    }
}

/// Convenience wrapper: parses `pattern` and matches it against `path`
/// in one call. Returns an error only if the pattern itself is malformed.
pub fn matches(pattern: &str, path: &str) -> Result<Option<Vec<(String, String)>>, PatternError> {
    let segments = parse_pattern(pattern)?;
    let path_segments = split_path(path);
    Ok(match_segments(&segments, &path_segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_static_and_param_segments() {
        let segments = parse_pattern("/users/:id/posts").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Static("users".to_string()),
                Segment::Param("id".to_string()),
                Segment::Static("posts".to_string()),
            ]
        );
    }

    #[test]
    fn rejects_wildcard_not_in_last_position() {
        let err = parse_pattern("/files/*rest/extra").unwrap_err();
        assert!(err.message.contains("must be last"));
    }

    #[test]
    fn rejects_empty_segment() {
        let err = parse_pattern("/users//id").unwrap_err();
        assert!(err.message.contains("empty segment"));
    }

    #[test]
    fn matches_static_route() {
        let result = matches("/health", "/health").unwrap();
        assert_eq!(result, Some(Vec::new()));
    }

    #[test]
    fn matches_param_and_captures_value() {
        let result = matches("/users/:id", "/users/42").unwrap();
        assert_eq!(result, Some(vec![("id".to_string(), "42".to_string())]));
    }

    #[test]
    fn rejects_mismatched_static_segment() {
        let result = matches("/users/:id", "/accounts/42").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn rejects_wrong_segment_count() {
        let result = matches("/users/:id", "/users/42/posts").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn wildcard_captures_remaining_path() {
        let result = matches("/files/*rest", "/files/2026/08/report.pdf").unwrap();
        assert_eq!(
            result,
            Some(vec![("rest".to_string(), "2026/08/report.pdf".to_string())])
        );
    }

    #[test]
    fn wildcard_requires_at_least_one_segment() {
        let result = matches("/files/*rest", "/files").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn root_pattern_matches_root_path_only() {
        assert_eq!(matches("/", "/").unwrap(), Some(Vec::new()));
        assert_eq!(matches("/", "/anything").unwrap(), None);
    }

    #[test]
    fn optional_trailing_param_may_be_absent() {
        let result = matches("/users/:id/:format?", "/users/42").unwrap();
        assert_eq!(result, Some(vec![("id".to_string(), "42".to_string())]));
    }

    #[test]
    fn optional_trailing_param_captures_when_present() {
        let result = matches("/users/:id/:format?", "/users/42/json").unwrap();
        assert_eq!(
            result,
            Some(vec![
                ("id".to_string(), "42".to_string()),
                ("format".to_string(), "json".to_string()),
            ])
        );
    }

    #[test]
    fn optional_trailing_static_must_match_when_present() {
        assert_eq!(
            matches("/report/json?", "/report/json").unwrap(),
            Some(Vec::new())
        );
        assert_eq!(matches("/report/json?", "/report").unwrap(), Some(Vec::new()));
        assert_eq!(matches("/report/json?", "/report/xml").unwrap(), None);
    }

    #[test]
    fn later_optional_segment_cannot_match_without_earlier_one() {
        // Path only has one component left, so it satisfies ":a?" and
        // there's nothing left for ":b?" — it is skipped, not matched
        // out of order.
        let result = matches("/x/:a?/:b?", "/x/one").unwrap();
        assert_eq!(result, Some(vec![("a".to_string(), "one".to_string())]));
    }

    #[test]
    fn too_many_path_segments_after_optional_run_does_not_match() {
        let result = matches("/x/:a?", "/x/one/two").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn rejects_optional_segment_before_required_segment() {
        let err = parse_pattern("/users/:id?/posts").unwrap_err();
        assert!(err.message.contains("must be at the end"));
    }

    #[test]
    fn rejects_optional_wildcard() {
        let err = parse_pattern("/files/*rest?").unwrap_err();
        assert!(err.message.contains("cannot be optional"));
    }
}
