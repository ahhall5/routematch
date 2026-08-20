use std::fmt;

/// One piece of a parsed route pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
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
pub fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, PatternError> {
    let trimmed = pattern.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    if body.is_empty() {
        return Ok(Vec::new());
    }

    let raw_segments: Vec<&str> = body.split('/').collect();
    let mut segments = Vec::with_capacity(raw_segments.len());

    for (i, raw) in raw_segments.iter().enumerate() {
        if raw.is_empty() {
            return Err(PatternError {
                message: format!("empty segment in pattern '{}'", pattern),
            });
        }
        let is_last = i == raw_segments.len() - 1;

        if let Some(name) = raw.strip_prefix(':') {
            if name.is_empty() {
                return Err(PatternError {
                    message: format!("param segment missing a name in '{}'", pattern),
                });
            }
            segments.push(Segment::Param(name.to_string()));
        } else if let Some(name) = raw.strip_prefix('*') {
            if name.is_empty() {
                return Err(PatternError {
                    message: format!("wildcard segment missing a name in '{}'", pattern),
                });
            }
            if !is_last {
                return Err(PatternError {
                    message: format!("wildcard segment must be last in '{}'", pattern),
                });
            }
            segments.push(Segment::Wildcard(name.to_string()));
        } else {
            segments.push(Segment::Static(raw.to_string()));
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
pub fn match_segments(
    segments: &[Segment],
    path_segments: &[&str],
) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_iter = path_segments.iter();

    for (i, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Wildcard(name) => {
                let rest: Vec<&str> = path_iter.by_ref().collect();
                if rest.is_empty() {
                    return None;
                }
                params.push((name.clone(), rest.join("/")));
                return if i == segments.len() - 1 {
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
}
