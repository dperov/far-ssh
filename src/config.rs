use std::{collections::BTreeMap, path::Path};

pub fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub fn aliases() -> Vec<String> {
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return Vec::new();
    };
    let home = Path::new(&home);
    let root = home.join(".ssh");
    let mut aliases = BTreeMap::new();
    collect(&root.join("config"), &root, home, 0, &mut aliases);
    aliases.into_values().collect()
}

fn collect(
    path: &Path,
    root: &Path,
    home: &Path,
    depth: usize,
    aliases: &mut BTreeMap<String, String>,
) {
    if depth >= 16 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim_start();
        let end = line
            .find(|c: char| c == '=' || c.is_ascii_whitespace())
            .unwrap_or(line.len());
        let (keyword, args) = line.split_at(end);
        let args = args.trim_start().trim_start_matches('=');
        if keyword.eq_ignore_ascii_case("host") {
            for alias in words(args).into_iter().filter(|name| valid_alias(name)) {
                aliases.entry(alias.to_ascii_lowercase()).or_insert(alias);
            }
        } else if keyword.eq_ignore_ascii_case("include") {
            for pattern in words(args) {
                let pattern = if let Some(rest) = pattern.strip_prefix("~/") {
                    home.join(rest)
                } else {
                    root.join(pattern)
                };
                if let Some(pattern) = pattern.to_str() {
                    if let Ok(paths) = glob::glob(pattern) {
                        for path in paths.flatten() {
                            collect(&path, root, home, depth + 1, aliases);
                        }
                    }
                }
            }
        }
    }
}

fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars
                .peek()
                .is_some_and(|c| matches!(c, '\\' | '"' | '\'' | ' ' | '#')) =>
            {
                word.push(chars.next().unwrap())
            }
            c if Some(c) == quote => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            '#' if quote.is_none() => break,
            c if c.is_ascii_whitespace() && quote.is_none() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if quote.is_none() && !word.is_empty() {
        words.push(word);
    }
    words
}
