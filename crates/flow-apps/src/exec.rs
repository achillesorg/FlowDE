//! Shell-free `Exec=` handling.
//!
//! The Desktop Entry spec quoting rules are implemented directly: double
//! quotes group, backslash escapes `" ` $ \` inside quotes. No shell is ever
//! involved, so metacharacters (`;`, `|`, `$()`) are plain argument bytes.

use flow_common::{FlowError, Result};

/// Split an Exec string into argv (before field-code expansion).
pub fn tokenize(exec: &str) -> Result<Vec<String>> {
    let mut args: Vec<String> = vec![];
    let mut cur = String::new();
    let mut in_arg = false;
    let mut quoted = false;
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        if quoted {
            match c {
                '"' => quoted = false,
                '\\' => match it.next() {
                    Some(n @ ('"' | '`' | '$' | '\\')) => cur.push(n),
                    Some(n) => {
                        cur.push('\\');
                        cur.push(n)
                    }
                    None => return Err(FlowError::Invalid("dangling backslash".into())),
                },
                _ => cur.push(c),
            }
        } else if c == '"' {
            quoted = true;
            in_arg = true;
        } else if c == ' ' || c == '\t' {
            if in_arg {
                args.push(std::mem::take(&mut cur));
                in_arg = false;
            }
        } else {
            cur.push(c);
            in_arg = true;
        }
    }
    if quoted {
        return Err(FlowError::Invalid("unterminated quote".into()));
    }
    if in_arg {
        args.push(cur);
    }
    if args.is_empty() {
        return Err(FlowError::Invalid("empty Exec".into()));
    }
    Ok(args)
}

pub struct LaunchCtx<'a> {
    /// Files/URLs passed by the caller.
    pub uris: &'a [String],
    pub name: &'a str,
    pub icon: Option<&'a str>,
    pub desktop_path: Option<&'a str>,
}

/// Expand field codes into the final argv. Deprecated codes
/// (%d %D %n %N %v %m) expand to nothing; unknown codes are an error.
pub fn expand(tokens: &[String], ctx: &LaunchCtx) -> Result<Vec<String>> {
    let mut out = vec![];
    for t in tokens {
        match t.as_str() {
            "%f" => out.extend(ctx.uris.first().cloned()),
            "%u" => out.extend(ctx.uris.first().cloned()),
            "%F" | "%U" => out.extend(ctx.uris.iter().cloned()),
            "%i" => {
                if let Some(i) = ctx.icon {
                    out.push("--icon".into());
                    out.push(i.into());
                }
            }
            "%c" => out.push(ctx.name.into()),
            "%k" => out.extend(ctx.desktop_path.map(String::from)),
            "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            other => {
                // Embedded codes such as `--file=%f`.
                let mut s = String::new();
                let mut ch = other.chars();
                while let Some(c) = ch.next() {
                    if c != '%' {
                        s.push(c);
                        continue;
                    }
                    match ch.next() {
                        Some('%') => s.push('%'),
                        Some('f') | Some('u') => {
                            s.push_str(ctx.uris.first().map(String::as_str).unwrap_or(""))
                        }
                        Some('c') => s.push_str(ctx.name),
                        Some('k') => s.push_str(ctx.desktop_path.unwrap_or("")),
                        Some('d' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
                        Some(x) => {
                            return Err(FlowError::Invalid(format!("unknown field code %{x}")))
                        }
                        None => return Err(FlowError::Invalid("dangling %".into())),
                    }
                }
                out.push(s);
            }
        }
    }
    if out.is_empty() {
        return Err(FlowError::Invalid("empty command after expansion".into()));
    }
    Ok(out)
}

/// Convenience: tokenise + expand.
pub fn build_argv(exec: &str, ctx: &LaunchCtx) -> Result<Vec<String>> {
    expand(&tokenize(exec)?, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(uris: &'a [String]) -> LaunchCtx<'a> {
        LaunchCtx {
            uris,
            name: "App",
            icon: Some("ic"),
            desktop_path: Some("/a.desktop"),
        }
    }

    #[test]
    fn tokenizes_quotes() {
        assert_eq!(tokenize(r#"foo "a b" c"#).unwrap(), vec!["foo", "a b", "c"]);
        assert_eq!(
            tokenize(r#"foo "a\"b" "" x"#).unwrap(),
            vec!["foo", "a\"b", "", "x"]
        );
        assert_eq!(tokenize("  a   b ").unwrap(), vec!["a", "b"]);
        assert!(tokenize(r#"a "b"#).is_err());
        assert!(tokenize("   ").is_err());
    }

    #[test]
    fn metacharacters_are_literal() {
        let t = tokenize("sh -c ls;rm$(x)|y").unwrap();
        assert_eq!(t, vec!["sh", "-c", "ls;rm$(x)|y"]);
    }

    #[test]
    fn expands_codes() {
        let u = vec!["a.txt".to_string(), "b.txt".to_string()];
        assert_eq!(
            build_argv("ed %F", &ctx(&u)).unwrap(),
            vec!["ed", "a.txt", "b.txt"]
        );
        assert_eq!(build_argv("ed %f", &ctx(&u)).unwrap(), vec!["ed", "a.txt"]);
        assert_eq!(build_argv("ed %U", &ctx(&[])).unwrap(), vec!["ed"]);
        assert_eq!(
            build_argv("ed %i %c", &ctx(&[])).unwrap(),
            vec!["ed", "--icon", "ic", "App"]
        );
        assert_eq!(
            build_argv("ed --file=%f 100%%", &ctx(&u)).unwrap(),
            vec!["ed", "--file=a.txt", "100%"]
        );
        assert_eq!(build_argv("ed %n %d", &ctx(&[])).unwrap(), vec!["ed"]);
        assert!(build_argv("ed %z", &ctx(&[])).is_err());
    }

    #[test]
    fn uri_with_shell_chars_stays_single_arg() {
        let u = vec!["a; rm -rf ~".to_string()];
        assert_eq!(
            build_argv("v %f", &ctx(&u)).unwrap(),
            vec!["v", "a; rm -rf ~"]
        );
    }
}
