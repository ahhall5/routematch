# routematch

A command line tool for checking a URL path against a list of route
patterns, without wiring anything into an actual web framework.

I keep ending up with route tables (in nginx configs, in API gateway
rules, in some framework's router) where it's not obvious which rule a
given path will actually hit, especially once wildcards and overlapping
prefixes get involved. This lets you paste the patterns into a text file
and check paths against them directly.

## Pattern syntax

- `users` — a literal path segment
- `:id` — matches exactly one segment, captured under the name `id`
- `*rest` — matches one or more remaining segments, captured joined by
  `/`; only valid as the last segment in a pattern
- `:id?` or `users?` — an optional segment (param or literal); the path
  may end before it. Optional segments must all sit at the end of the
  pattern, after every required segment, and a wildcard can't be made
  optional (it already matches a variable number of segments). If a
  pattern has more than one trailing optional segment, the path can
  only supply them in order — you can't skip one and still match the
  next, e.g. `/x/:a?/:b?` against `/x/one` captures `a=one` and leaves
  `b` unset, it never treats `one` as `b`.
- `:id(\d+)` — a param constrained to values matching the pattern in
  parens; the segment only matches if the whole component satisfies it.
  Combine with `?` for an optional constrained param, e.g. `:id(\d+)?`.
  The constraint language is a small subset of regex: literal
  characters, `.` (any character), `\d`/`\D`/`\w`/`\W`/`\s`/`\S`,
  `[a-z0-9]`-style character classes (with `^` negation and `a-z`
  ranges), and the `*`, `+`, `?` quantifiers. There's no grouping,
  alternation, or `{n,m}` repetition — enough to constrain a single
  segment (`:id(\d+)`, `:slug([a-z0-9-]+)`), not to embed a sub-router.

## Routes file

One route per line, as `name = pattern`. The name is just a label for
the output; blank lines and lines starting with `#` are ignored.

```
# api.routes
user_show   = /users/:id(\d+)
user_posts  = /users/:id/posts/:post_id
static_file = /assets/*path
health      = /health
report      = /report/:id/:format?
```

## Usage

```
$ routematch api.routes /users/42
user_show (/users/:id(\d+)) id=42

$ routematch api.routes /users/abc
no route matches '/users/abc'

$ routematch api.routes /users/42/posts/7
user_posts (/users/:id/posts/:post_id) id=42 post_id=7

$ routematch api.routes /assets/css/site.css
static_file (/assets/*path) path=css/site.css

$ routematch api.routes /report/42
report (/report/:id/:format?) id=42

$ routematch api.routes /report/42/json
report (/report/:id/:format?) id=42 format=json

$ routematch api.routes /unknown
no route matches '/unknown'
```

If a path matches more than one route, every matching route is printed,
in the order it appears in the file — useful for catching accidental
overlaps between rules.

### Overlap warnings

On every run, routematch also checks the whole file for pairs of
patterns that could both match *some* path, regardless of the path you
actually asked about, and prints a warning for each pair to stderr. For
example, adding `user_active = /users/active` to `api.routes` above
would warn on every run, since `active` also satisfies `:id`:

```
$ routematch api.routes /users/42
warning: routes 'user_show' (/users/:id(\d+)) and 'user_active' (/users/active) may both match the same path
user_show (/users/:id(\d+)) id=42
```

This catches routes that shadow each other even when the specific path
you're testing only happens to hit one of them. It's a static property
of the file, so the same warnings appear no matter what path you pass.
The check doesn't evaluate constraints, so it treats `:id(\d+)` the
same as an unconstrained `:id` here — `active` would never actually
satisfy `\d+`, but the warning fires anyway. That's deliberate: it
would rather warn about an overlap a constraint happens to rule out
than stay quiet about one it doesn't.

The overlap check is quadratic in the number of routes, which is fine
for the routes files this was built for but noticeable once a file
has thousands of lines and you're just checking a path against it
repeatedly. Pass `--path-only` to skip it and only match:

```
$ routematch api.routes /users/42 --path-only
user_show (/users/:id(\d+)) id=42
```

### JSON output

Pass `--json` (in either argument position) to get matches as a JSON
array instead of text, for feeding into another script:

```
$ routematch api.routes /users/42/posts/7 --json
[{"name":"user_posts","pattern":"/users/:id/posts/:post_id","params":{"id":"42","post_id":"7"}}]

$ routematch api.routes /unknown --json
[]
```

Malformed patterns in the routes file are still reported on stderr, not
folded into the JSON, so a caller reading stdout doesn't have to filter
them out. The exit code is unchanged: 1 if nothing matched, 0 otherwise.

### Explaining non-matches

Pass `--explain` to print, for every route that did *not* match, the
reason why — which segment disagreed and how. Every non-matching
route in the file gets its own line:

```
$ routematch api.routes /users/abc --explain
user_show (/users/:id(\d+)) did not match: segment 2 ('abc') does not satisfy the constraint on ':id' (\d+)
user_posts (/users/:id/posts/:post_id) did not match: path has only 2 segment(s), but the pattern requires at least 4
static_file (/assets/*path) did not match: segment 1 is 'users', expected literal 'assets'
health (/health) did not match: segment 1 is 'users', expected literal 'health'
report (/report/:id/:format?) did not match: segment 1 is 'users', expected literal 'report'
```

This goes to stderr alongside pattern errors and overlap warnings, so
it doesn't affect `--json` output on stdout. It's meant for the case
where a path you expected to match doesn't, and it's not obvious which
route was closest or why it fell short.

## Building

Standard library only, no external dependencies:

```
cargo build --release
```

## Design note

Everything that actually decides whether a pattern matches a path lives
in `src/router.rs` as plain functions with no I/O: `parse_pattern`,
`split_path`, and `match_segments`. `src/constraint.rs` holds the small
matcher for `:id(\d+)`-style param constraints, kept separate since it's
a self-contained grammar with its own parser and backtracking matcher.
`src/main.rs` only handles reading the file and printing output. That
split is what makes the matching logic easy to unit test with
`cargo test` — no fixtures, no filesystem, just strings in and values
out.
