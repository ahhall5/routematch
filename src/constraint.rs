/// A small, purpose-built matcher for the constraint expressions that can
/// follow a param, e.g. `:id(\d+)`. This is not a general regex engine:
/// there are no groups, no alternation, and no `{n,m}` repetition counts.
/// It supports exactly what's useful for constraining a single path
/// segment - literal characters, `.`, shorthand classes (`\d`, `\w`, `\s`
/// and their negations), `[...]` character classes with ranges, and the
/// `*`, `+`, `?` quantifiers. A hand-rolled backtracking matcher over that
/// small grammar is easy to get right without pulling in a dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    source: String,
    atoms: Vec<Atom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Atom {
    class: CharClass,
    quantifier: Quantifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quantifier {
    One,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CharClass {
    Any,
    Char(char),
    Digit,
    NonDigit,
    Word,
    NonWord,
    Whitespace,
    NonWhitespace,
    Set { negated: bool, items: Vec<SetItem> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SetItem {
    Char(char),
    Range(char, char),
}

impl Constraint {
    /// Parses a constraint body, i.e. the text between the parens in
    /// `:id(\d+)`. The whole value of the matched path segment must
    /// satisfy the pattern end to end; there's no partial matching.
    pub fn parse(source: &str) -> Result<Constraint, String> {
        if source.is_empty() {
            return Err("constraint pattern is empty".to_string());
        }

        let chars: Vec<char> = source.chars().collect();
        let mut atoms = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let class = match chars[i] {
                '\\' => {
                    i += 1;
                    let escaped = *chars
                        .get(i)
                        .ok_or_else(|| format!("dangling escape in constraint '{}'", source))?;
                    i += 1;
                    escape_class(escaped, source)?
                }
                '.' => {
                    i += 1;
                    CharClass::Any
                }
                '[' => {
                    i += 1;
                    parse_set(&chars, &mut i, source)?
                }
                '(' | ')' | '|' => {
                    return Err(format!(
                        "unsupported constraint syntax '{}' in '{}' (groups and alternation aren't supported)",
                        chars[i], source
                    ));
                }
                c => {
                    i += 1;
                    CharClass::Char(c)
                }
            };

            let quantifier = match chars.get(i) {
                Some('*') => {
                    i += 1;
                    Quantifier::ZeroOrMore
                }
                Some('+') => {
                    i += 1;
                    Quantifier::OneOrMore
                }
                Some('?') => {
                    i += 1;
                    Quantifier::ZeroOrOne
                }
                _ => Quantifier::One,
            };

            atoms.push(Atom { class, quantifier });
        }

        Ok(Constraint {
            source: source.to_string(),
            atoms,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether `value` matches this constraint in its entirety.
    pub fn is_match(&self, value: &str) -> bool {
        let chars: Vec<char> = value.chars().collect();
        match_from(&self.atoms, &chars)
    }
}

fn escape_class(escaped: char, source: &str) -> Result<CharClass, String> {
    match escaped {
        'd' => Ok(CharClass::Digit),
        'D' => Ok(CharClass::NonDigit),
        'w' => Ok(CharClass::Word),
        'W' => Ok(CharClass::NonWord),
        's' => Ok(CharClass::Whitespace),
        'S' => Ok(CharClass::NonWhitespace),
        c if c.is_alphanumeric() => Err(format!(
            "unsupported escape '\\{}' in constraint '{}'",
            c, source
        )),
        c => Ok(CharClass::Char(c)),
    }
}

/// Parses a `[...]` character class body, with `i` positioned just past
/// the opening `[`. Advances `i` past the closing `]`.
fn parse_set(chars: &[char], i: &mut usize, source: &str) -> Result<CharClass, String> {
    let negated = chars.get(*i) == Some(&'^');
    if negated {
        *i += 1;
    }

    let mut items = Vec::new();
    loop {
        match chars.get(*i) {
            None => return Err(format!("unterminated '[' in constraint '{}'", source)),
            Some(']') => {
                *i += 1;
                break;
            }
            Some(_) => {
                let lo = read_set_char(chars, i, source)?;
                if chars.get(*i) == Some(&'-') && chars.get(*i + 1) != Some(&']') {
                    *i += 1;
                    let hi = read_set_char(chars, i, source)?;
                    if hi < lo {
                        return Err(format!(
                            "invalid range '{}-{}' in constraint '{}'",
                            lo, hi, source
                        ));
                    }
                    items.push(SetItem::Range(lo, hi));
                } else {
                    items.push(SetItem::Char(lo));
                }
            }
        }
    }

    if items.is_empty() {
        return Err(format!("empty character class in constraint '{}'", source));
    }

    Ok(CharClass::Set { negated, items })
}

fn read_set_char(chars: &[char], i: &mut usize, source: &str) -> Result<char, String> {
    let c = chars[*i];
    if c == '\\' {
        *i += 1;
        let escaped = *chars
            .get(*i)
            .ok_or_else(|| format!("dangling escape in constraint '{}'", source))?;
        *i += 1;
        Ok(escaped)
    } else {
        *i += 1;
        Ok(c)
    }
}

/// Backtracking match of `atoms` against `chars`, anchored at both ends.
/// Quantifiers are greedy: the longest run is tried first, falling back
/// to shorter runs only if the rest of the pattern can't follow it.
fn match_from(atoms: &[Atom], chars: &[char]) -> bool {
    let Some((atom, rest_atoms)) = atoms.split_first() else {
        return chars.is_empty();
    };

    match atom.quantifier {
        Quantifier::One => match chars.split_first() {
            Some((&c, rest_chars)) if class_matches(&atom.class, c) => {
                match_from(rest_atoms, rest_chars)
            }
            _ => false,
        },
        Quantifier::ZeroOrOne => {
            if let Some((&c, rest_chars)) = chars.split_first() {
                if class_matches(&atom.class, c) && match_from(rest_atoms, rest_chars) {
                    return true;
                }
            }
            match_from(rest_atoms, chars)
        }
        Quantifier::ZeroOrMore | Quantifier::OneOrMore => {
            let mut max = 0;
            while max < chars.len() && class_matches(&atom.class, chars[max]) {
                max += 1;
            }
            let min = if atom.quantifier == Quantifier::OneOrMore {
                1
            } else {
                0
            };
            if max < min {
                return false;
            }
            (min..=max).rev().any(|n| match_from(rest_atoms, &chars[n..]))
        }
    }
}

fn class_matches(class: &CharClass, c: char) -> bool {
    match class {
        CharClass::Any => true,
        CharClass::Char(expected) => c == *expected,
        CharClass::Digit => c.is_ascii_digit(),
        CharClass::NonDigit => !c.is_ascii_digit(),
        CharClass::Word => c.is_ascii_alphanumeric() || c == '_',
        CharClass::NonWord => !(c.is_ascii_alphanumeric() || c == '_'),
        CharClass::Whitespace => c.is_whitespace(),
        CharClass::NonWhitespace => !c.is_whitespace(),
        CharClass::Set { negated, items } => {
            let hit = items.iter().any(|item| match item {
                SetItem::Char(x) => c == *x,
                SetItem::Range(lo, hi) => c >= *lo && c <= *hi,
            });
            hit != *negated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, value: &str) -> bool {
        Constraint::parse(pattern).unwrap().is_match(value)
    }

    #[test]
    fn digit_class_matches_only_digits() {
        assert!(matches(r"\d+", "42"));
        assert!(!matches(r"\d+", "4a"));
        assert!(!matches(r"\d+", ""));
    }

    #[test]
    fn literal_chars_must_match_exactly() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abc", "abcd"));
        assert!(!matches("abc", "ab"));
    }

    #[test]
    fn word_class_matches_letters_digits_and_underscore() {
        assert!(matches(r"\w+", "user_42"));
        assert!(!matches(r"\w+", "user-42"));
    }

    #[test]
    fn char_class_with_range_matches() {
        assert!(matches("[a-f0-9]+", "cafe0"));
        assert!(!matches("[a-f0-9]+", "cafeg"));
    }

    #[test]
    fn negated_char_class_excludes_members() {
        assert!(matches("[^0-9]+", "abc"));
        assert!(!matches("[^0-9]+", "ab3"));
    }

    #[test]
    fn optional_quantifier_allows_zero_or_one() {
        assert!(matches(r"colou?r", "color"));
        assert!(matches(r"colou?r", "colour"));
        assert!(!matches(r"colou?r", "colouur"));
    }

    #[test]
    fn backtracks_when_greedy_run_overshoots() {
        // ".*" would happily eat the whole string, but has to give back
        // the trailing "ab" for the rest of the pattern to match.
        assert!(matches(".*ab", "xxxab"));
        assert!(!matches(".*ab", "xxxa"));
    }

    #[test]
    fn rejects_dangling_escape() {
        assert!(Constraint::parse("abc\\").is_err());
    }

    #[test]
    fn rejects_unterminated_char_class() {
        assert!(Constraint::parse("[a-z").is_err());
    }

    #[test]
    fn rejects_empty_char_class() {
        assert!(Constraint::parse("[]").is_err());
    }

    #[test]
    fn rejects_groups_and_alternation() {
        assert!(Constraint::parse("(abc)").is_err());
        assert!(Constraint::parse("a|b").is_err());
    }

    #[test]
    fn rejects_empty_pattern() {
        assert!(Constraint::parse("").is_err());
    }
}
