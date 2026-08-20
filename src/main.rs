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
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: routematch <routes-file> <path>");
        process::exit(2);
    }

    let routes_path = &args[1];
    let path = &args[2];

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

    let mut found = false;
    for route in &routes {
        match matches(&route.pattern, path) {
            Ok(Some(params)) => {
                found = true;
                print_match(&route.name, &route.pattern, &params);
            }
            Ok(None) => {}
            Err(e) => report_pattern_error(&route.name, &e),
        }
    }

    if !found {
        println!("no route matches '{}'", path);
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

fn report_pattern_error(name: &str, err: &PatternError) {
    eprintln!("skipping route '{}': {}", name, err);
}
