mod router;

use router::{matches, PatternError};
use std::env;
use std::fs;
use std::process;

struct Route {
    name: String,
    pattern: String,
}

/// Turns a routes file into a list of routes. Lines are "name = pattern",
/// or just "pattern" if no name is given. Blank lines and lines starting
/// with '#' are skipped.
fn parse_routes_file(contents: &str) -> Vec<Route> {
    contents
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.split_once('=') {
            Some((name, pattern)) => Route {
                name: name.trim().to_string(),
                pattern: pattern.trim().to_string(),
            },
            None => Route {
                name: line.to_string(),
                pattern: line.to_string(),
            },
        })
        .collect()
}

fn main() {
    let mut json_output = false;
    let mut positional: Vec<String> = Vec::new();
    for arg in env::args().skip(1) {
        if arg == "--json" {
            json_output = true;
        } else {
            positional.push(arg);
        }
    }
    if positional.len() != 2 {
        eprintln!("usage: routematch <routes-file> <path> [--json]");
        process::exit(2);
    }

    let routes_path = &positional[0];
    let path = &positional[1];

    let contents = match fs::read_to_string(routes_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error reading '{}': {}", routes_path, e);
            process::exit(1);
        }
    };

    let routes = parse_routes_file(&contents);
    if routes.is_empty() {
        eprintln!("no routes found in '{}'", routes_path);
        process::exit(1);
    }

    let mut hits: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
    for route in &routes {
        match matches(&route.pattern, path) {
            Ok(Some(params)) => hits.push((route, params)),
            Ok(None) => {}
            // Pattern errors always go to stderr, even in --json mode, so
            // a script parsing stdout doesn't have to account for them.
            Err(e) => report_pattern_error(&route.name, &e),
        }
    }

    if json_output {
        print_matches_json(&hits);
    } else if hits.is_empty() {
        println!("no route matches '{}'", path);
    } else {
        for (route, params) in &hits {
            print_match(&route.name, &route.pattern, params);
        }
    }

    if hits.is_empty() {
        process::exit(1);
    }
}

fn print_match(name: &str, pattern: &str, params: &[(String, String)]) {
    if params.is_empty() {
        println!("{} ({})", name, pattern);
    } else {
        let rendered: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        println!("{} ({}) {}", name, pattern, rendered.join(" "));
    }
}

fn print_matches_json(hits: &[(&Route, Vec<(String, String)>)]) {
    let mut out = String::from("[");
    for (i, (route, params)) in hits.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        out.push_str(&json_string(&route.name));
        out.push_str(",\"pattern\":");
        out.push_str(&json_string(&route.pattern));
        out.push_str(",\"params\":{");
        for (j, (key, value)) in params.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&json_string(key));
            out.push(':');
            out.push_str(&json_string(value));
        }
        out.push_str("}}");
    }
    out.push(']');
    println!("{}", out);
}

/// Renders a string as a JSON string literal, escaping the characters
/// that would otherwise break the surrounding array/object syntax or
/// produce invalid JSON.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn report_pattern_error(name: &str, err: &PatternError) {
    eprintln!("skipping route '{}': {}", name, err);
}
