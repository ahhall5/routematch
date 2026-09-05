use crate::constraint::Constraint;
use std::fmt;

/// One piece of a parsed route pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Static(String),
    Param(String),
    /// A param constrained to values matching a pattern, e.g.
    /// `:id(\d+)`. The path component must match the whole constraint,
    /// not just contain a match somewhere in it.
    ConstrainedParam(String, Constraint),
    Wildcard(String),
    /// A static segment suffixed with '?'. Only valid as part of a
    /// trailing run of optional segments.
    OptionalStatic(String),
    /// A param segment suffixed with '?'. Only valid as part of a
    /// trailing run of optional segments.
    OptionalParam(String),
    /// A constrained param suffixed with '?', combining the two.
    OptionalConstrainedParam(String, Constraint),
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
///
/// A param may carry a constraint in parens, e.g. ":id(\d+)", requiring
/// the matched component to satisfy that pattern (see `constraint`
/// module) rather than just being any single segment. The constraint
/// suffix can combine with the trailing '?' for an optional constrained
/// param, e.g. ":id(\d+)?".
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
            let (param_name, constraint_src) = split_constraint(name, pattern)?;
            if param_name.is_empty() {
                return Err(PatternError {
                    message: format!("param segment missing a name in '{}'", pattern),
                });
            }
            segments.push(match constraint_src {
                Some(constraint_src) => {
                    let constraint = Constraint::parse(constraint_src).map_err(|message| {
                        PatternError {
                            message: format!(
                                "in constraint for ':{}' in pattern '{}': {}",
                                param_name, pattern, message
                            ),
                        }
                    })?;
                    if optional {
                        Segment::OptionalConstrainedParam(param_name.to_string(), constraint)
                    } else {
                        Segment::ConstrainedParam(param_name.to_string(), constraint)
                    }
                }
                None if optional => Segment::OptionalParam(param_name.to_string()),
                None => Segment::Param(param_name.to_string()),
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
            Segment::OptionalStatic(_)
            | Segment::OptionalParam(_)
            | Segment::OptionalConstrainedParam(_, _) => seen_optional = true,
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

/// Splits a param name like "id(\d+)" into the name and the constraint
/// body, if any. `name` has already had the leading ':' and any trailing
/// '?' stripped.
fn split_constraint<'a>(
    name: &'a str,
    pattern: &str,
) -> Result<(&'a str, Option<&'a str>), PatternError> {
    match name.find('(') {
        None => Ok((name, None)),
        Some(open) => {
            if !name.ends_with(')') {
                return Err(PatternError {
                    message: format!(
                        "unterminated constraint in ':{}' in pattern '{}'",
                        name, pattern
                    ),
                });
            }
            Ok((&name[..open], Some(&name[open + 1..name.len() - 1])))
        }
    }
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

/// Splits segments into the leading required run and the trailing
/// optional run. `parse_pattern` already guarantees every optional
/// segment sits at the end, so a single split point is enough.
fn split_optional(segments: &[Segment]) -> (&[Segment], &[Segment]) {
    let optional_start = segments
        .iter()
        .position(|s| {
            matches!(
                s,
                Segment::OptionalStatic(_)
                    | Segment::OptionalParam(_)
                    | Segment::OptionalConstrainedParam(_, _)
            )
        })
        .unwrap_or(segments.len());
    segments.split_at(optional_start)
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
    let (required, optional) = split_optional(segments);

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
            Segment::ConstrainedParam(name, constraint) => {
                let value = path_iter.next()?;
                if !constraint.is_match(value) {
                    return None;
                }
                params.push((name.clone(), value.to_string()));
            }
            Segment::Static(expected) => {
                let value = path_iter.next()?;
                if value != expected {
                    return None;
                }
            }
            Segment::OptionalStatic(_)
            | Segment::OptionalParam(_)
            | Segment::OptionalConstrainedParam(_, _) => unreachable!(
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
            Segment::OptionalConstrainedParam(name, constraint) => {
                if !constraint.is_match(value) {
                    return None;
                }
                params.push((name.clone(), value.to_string()));
            }
            Segment::OptionalStatic(expected) => {
                if value != expected {
                    return None;
                }
            }
            _ => unreachable!(
                "`optional` only ever holds OptionalStatic/OptionalParam/OptionalConstrainedParam"
            ),
        }
    }

    if path_iter.next().is_some() {
        None
    } else {
        Some(params)
    }
}

/// Explains why `segments` failed to match `path_segments`. Meant to be
/// called after `match_segments` already returned `None` for the same
/// inputs; it walks the same required/optional structure and stops at
/// the first point of disagreement, since that's the thing a person
/// staring at a routes file actually wants to know first.
pub fn explain_mismatch(segments: &[Segment], path_segments: &[&str]) -> String {
    let (required, optional) = split_optional(segments);
    let mut idx = 0;

    for segment in required {
        if let Segment::Wildcard(name) = segment {
            if idx >= path_segments.len() {
                return format!(
                    "wildcard ':{}' needs at least one path segment after position {}, but the path ends there",
                    name, idx
                );
            }
            return "pattern matches".to_string();
        }

        if idx >= path_segments.len() {
            return format!(
                "path has only {} segment(s), but the pattern requires at least {}",
                path_segments.len(),
                required.len()
            );
        }
        let value = path_segments[idx];
        match segment {
            Segment::Static(expected) => {
                if value != expected {
                    return format!(
                        "segment {} is '{}', expected literal '{}'",
                        idx + 1,
                        value,
                        expected
                    );
                }
            }
            Segment::ConstrainedParam(name, constraint) => {
                if !constraint.is_match(value) {
                    return format!(
                        "segment {} ('{}') does not satisfy the constraint on ':{}' ({})",
                        idx + 1,
                        value,
                        name,
                        constraint.source()
                    );
                }
            }
            Segment::Param(_) => {}
            Segment::Wildcard(_) => unreachable!("handled above"),
            Segment::OptionalStatic(_)
            | Segment::OptionalParam(_)
            | Segment::OptionalConstrainedParam(_, _) => {
                unreachable!("optional segments never appear in the required run")
            }
        }
        idx += 1;
    }

    for segment in optional {
        if idx >= path_segments.len() {
            break;
        }
        let value = path_segments[idx];
        match segment {
            Segment::OptionalStatic(expected) => {
                if value != expected {
                    return format!(
                        "segment {} is '{}', expected optional literal '{}'",
                        idx + 1,
                        value,
                        expected
                    );
                }
            }
            Segment::OptionalConstrainedParam(name, constraint) => {
                if !constraint.is_match(value) {
                    return format!(
                        "segment {} ('{}') does not satisfy the constraint on ':{}' ({})",
                        idx + 1,
                        value,
                        name,
                        constraint.source()
                    );
                }
            }
            Segment::OptionalParam(_) => {}
            _ => unreachable!("`optional` only ever holds optional segments"),
        }
        idx += 1;
    }

    if idx < path_segments.len() {
        format!(
            "path has {} extra segment(s) beyond what the pattern accounts for",
            path_segments.len() - idx
        )
    } else {
        "pattern matches".to_string()
    }
}

/// Convenience wrapper: parses `pattern` and matches it against `path`
/// in one call. Returns an error only if the pattern itself is malformed.
pub fn matches(pattern: &str, path: &str) -> Result<Option<Vec<(String, String)>>, PatternError> {
    let segments = parse_pattern(pattern)?;
    let path_segments = split_path(path);
    Ok(match_segments(&segments, &path_segments))
}

/// What a given position in a pattern requires of a path component, for
/// the purposes of checking whether two patterns could ever match the
/// same path. `Static` values must match literally; everything else
/// (params, constrained params, optional params, and anything a
/// wildcard absorbs) matches any single component, so it's collapsed
/// into `Any`. A constraint might in fact rule out a particular literal,
/// but checking that would mean running the constraint matcher here, so
/// this stays a conservative check: it can warn about an overlap that a
/// constraint actually prevents, never miss a real one.
enum PosKind<'a> {
    Static(&'a str),
    Any,
}

/// The number of path components a pattern can match: `min` required,
/// `max` bounded unless the pattern ends in a wildcard.
fn length_range(required_len: usize, has_wildcard: bool, optional_len: usize) -> (usize, Option<usize>) {
    if has_wildcard {
        (required_len, None)
    } else {
        (required_len, Some(required_len + optional_len))
    }
}

/// What position `i` in a pattern requires, given its required/optional
/// split. A trailing wildcard absorbs its own position and everything
/// after it. Querying a position past a non-wildcard pattern's maximum
/// length is meaningless for the caller's purposes, so it falls back to
/// `Any` rather than asserting; `patterns_overlap` never does this since
/// it bounds `i` by each pattern's own reachable length.
fn kind_at<'a>(required: &'a [Segment], optional: &'a [Segment], i: usize) -> PosKind<'a> {
    if let Some(Segment::Wildcard(_)) = required.last() {
        if i >= required.len() - 1 {
            return PosKind::Any;
        }
    }
    if let Some(segment) = required.get(i) {
        return match segment {
            Segment::Static(s) => PosKind::Static(s.as_str()),
            Segment::Param(_) | Segment::Wildcard(_) | Segment::ConstrainedParam(_, _) => {
                PosKind::Any
            }
            Segment::OptionalStatic(_)
            | Segment::OptionalParam(_)
            | Segment::OptionalConstrainedParam(_, _) => {
                unreachable!("optional segments never appear in the required run")
            }
        };
    }
    match optional.get(i - required.len()) {
        Some(Segment::OptionalStatic(s)) => PosKind::Static(s.as_str()),
        Some(Segment::OptionalParam(_)) | Some(Segment::OptionalConstrainedParam(_, _)) | None => {
            PosKind::Any
        }
        Some(_) => unreachable!(
            "only OptionalStatic/OptionalParam/OptionalConstrainedParam appear in the optional run"
        ),
    }
}

/// Reports whether some path could match both patterns at once. This is
/// a static check over the pattern text, independent of any particular
/// path: it exists to flag routes files where two rules could both fire
/// for the same request, which is easy to introduce by accident once a
/// file has more than a handful of routes.
///
/// Two patterns overlap when their possible path lengths intersect and,
/// at every position within that overlap, neither requires a literal
/// value the other rules out. Params, optional params, and wildcards
/// place no constraint on a component's value, so they never block an
/// overlap by themselves — only two different literals at the same
/// position do.
pub fn patterns_overlap(a: &[Segment], b: &[Segment]) -> bool {
    let (required_a, optional_a) = split_optional(a);
    let (required_b, optional_b) = split_optional(b);
    let wildcard_a = matches!(required_a.last(), Some(Segment::Wildcard(_)));
    let wildcard_b = matches!(required_b.last(), Some(Segment::Wildcard(_)));

    let (min_a, max_a) = length_range(required_a.len(), wildcard_a, optional_a.len());
    let (min_b, max_b) = length_range(required_b.len(), wildcard_b, optional_b.len());
    if min_a > max_b.unwrap_or(usize::MAX) || min_b > max_a.unwrap_or(usize::MAX) {
        return false;
    }

    // Beyond this many positions, any still-overlapping length is only
    // adding wildcard-absorbed or param components, which never rule
    // an overlap out — so there's no need to check further.
    let check_upper = match (wildcard_a, wildcard_b) {
        (false, false) => max_a.unwrap().min(max_b.unwrap()),
        (true, false) => max_b.unwrap(),
        (false, true) => max_a.unwrap(),
        (true, true) => required_a.len().max(required_b.len()),
    };

    for i in 0..check_upper {
        if let (PosKind::Static(x), PosKind::Static(y)) = (
            kind_at(required_a, optional_a, i),
            kind_at(required_b, optional_b, i),
        ) {
            if x != y {
                return false;
            }
        }
    }
    true
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

    #[test]
    fn constrained_param_matches_only_conforming_values() {
        let result = matches(r"/users/:id(\d+)", "/users/42").unwrap();
        assert_eq!(result, Some(vec![("id".to_string(), "42".to_string())]));
        assert_eq!(matches(r"/users/:id(\d+)", "/users/abc").unwrap(), None);
    }

    #[test]
    fn optional_constrained_param_may_be_absent_or_conform() {
        assert_eq!(
            matches(r"/users/:id(\d+)?", "/users").unwrap(),
            Some(Vec::new())
        );
        assert_eq!(
            matches(r"/users/:id(\d+)?", "/users/42").unwrap(),
            Some(vec![("id".to_string(), "42".to_string())])
        );
        assert_eq!(matches(r"/users/:id(\d+)?", "/users/abc").unwrap(), None);
    }

    #[test]
    fn rejects_unterminated_constraint() {
        let err = parse_pattern(r"/users/:id(\d+").unwrap_err();
        assert!(err.message.contains("unterminated constraint"));
    }

    #[test]
    fn rejects_invalid_constraint_body() {
        let err = parse_pattern(r"/users/:id()").unwrap_err();
        assert!(err.message.contains("constraint pattern is empty"));
    }

    #[test]
    fn constrained_param_overlaps_with_unconstrained_param() {
        // The overlap check doesn't run the constraint matcher, so it
        // conservatively treats this as an overlap even though "abc"
        // would never satisfy `\d+`.
        assert!(overlap(r"/users/:id(\d+)", "/users/:name"));
    }

    fn overlap(a: &str, b: &str) -> bool {
        patterns_overlap(&parse_pattern(a).unwrap(), &parse_pattern(b).unwrap())
    }

    #[test]
    fn identical_patterns_overlap() {
        assert!(overlap("/users/:id", "/users/:id"));
    }

    #[test]
    fn param_overlaps_static_at_same_position() {
        // Both match "/users/42".
        assert!(overlap("/users/:id", "/users/42"));
    }

    #[test]
    fn different_statics_at_same_position_do_not_overlap() {
        assert!(!overlap("/users/:id", "/accounts/:id"));
    }

    #[test]
    fn different_lengths_do_not_overlap() {
        assert!(!overlap("/users/:id", "/users/:id/posts"));
    }

    #[test]
    fn wildcard_overlaps_longer_static_path() {
        // "/assets/logo.png" matches both.
        assert!(overlap("/assets/*path", "/assets/logo.png"));
    }

    #[test]
    fn wildcard_does_not_overlap_shorter_path() {
        // The wildcard needs at least one component past "/assets".
        assert!(!overlap("/assets/*path", "/assets"));
    }

    #[test]
    fn two_wildcards_on_different_prefixes_do_not_overlap() {
        assert!(!overlap("/assets/*path", "/uploads/*path"));
    }

    #[test]
    fn optional_trailing_segment_overlaps_shorter_required_route() {
        // "/report/42" matches both.
        assert!(overlap("/report/:id", "/report/:id/:format?"));
    }

    #[test]
    fn optional_static_mismatch_does_not_block_shorter_overlap() {
        // Both match "/report/42"; only the optional tail differs.
        assert!(overlap("/report/:id", "/report/:id/json?"));
    }

    fn explain(pattern: &str, path: &str) -> String {
        let segments = parse_pattern(pattern).unwrap();
        explain_mismatch(&segments, &split_path(path))
    }

    #[test]
    fn explains_static_segment_mismatch() {
        assert_eq!(
            explain("/users/:id", "/accounts/42"),
            "segment 1 is 'accounts', expected literal 'users'"
        );
    }

    #[test]
    fn explains_too_few_path_segments() {
        assert_eq!(
            explain("/users/:id/posts", "/users/42"),
            "path has only 2 segment(s), but the pattern requires at least 3"
        );
    }

    #[test]
    fn explains_too_many_path_segments() {
        assert_eq!(
            explain("/users/:id", "/users/42/posts"),
            "path has 1 extra segment(s) beyond what the pattern accounts for"
        );
    }

    #[test]
    fn explains_constraint_failure() {
        assert_eq!(
            explain(r"/users/:id(\d+)", "/users/abc"),
            r"segment 2 ('abc') does not satisfy the constraint on ':id' (\d+)"
        );
    }

    #[test]
    fn explains_wildcard_needing_a_segment() {
        assert_eq!(
            explain("/files/*rest", "/files"),
            "wildcard ':rest' needs at least one path segment after position 1, but the path ends there"
        );
    }

    #[test]
    fn explains_optional_static_mismatch() {
        assert_eq!(
            explain("/report/json?", "/report/xml"),
            "segment 2 is 'xml', expected optional literal 'json'"
        );
    }

    #[test]
    fn explains_optional_constraint_failure() {
        assert_eq!(
            explain(r"/users/:id(\d+)?", "/users/abc"),
            r"segment 2 ('abc') does not satisfy the constraint on ':id' (\d+)"
        );
    }
}
