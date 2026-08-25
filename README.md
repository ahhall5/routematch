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

## Routes file

One route per line, as `name = pattern`. The name is just a label for
the output; blank lines and lines starting with `#` are ignored.

```
# api.routes
user_show   = /users/:id
user_posts  = /users/:id/posts/:post_id
static_file = /assets/*path
health      = /health
report      = /report/:id/:format?
```

## Usage

```
$ routematch api.routes /users/42
user_show (/users/:id) id=42

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

## Building

Standard library only, no external dependencies:

```
cargo build --release
```

## Design note

Everything that actually decides whether a pattern matches a path lives
in `src/router.rs` as plain functions with no I/O: `parse_pattern`,
`split_path`, and `match_segments`. `src/main.rs` only handles reading
the file and printing output. That split is what makes the matching
logic easy to unit test with `cargo test` — no fixtures, no filesystem,
just strings in and values out.
