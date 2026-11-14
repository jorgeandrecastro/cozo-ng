//! Decides when the text typed so far is a complete script that can be submitted.

struct Scan {
    depth: i32,
    in_string: bool,
}

fn scan(s: &str) -> Scan {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == '\\' {
                    chars.next();
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' | '`' => quote = Some(c),
                '#' => {
                    // comment until end of line
                    for n in chars.by_ref() {
                        if n == '\n' {
                            break;
                        }
                    }
                }
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    Scan { depth, in_string: quote.is_some() }
}

const CONTINUATIONS: [&str; 14] =
    [",", ":=", "<-", "<~", "|", "&&", "||", "+", "-", "*", "/", "=", "(", "["];

pub fn is_complete(input: &str) -> bool {
    let t = input.trim();
    if t.is_empty() {
        return true;
    }
    // a blank line submits whatever has been typed
    if input.ends_with('\n') {
        return true;
    }
    if t.starts_with('.') && !t.contains('\n') {
        return true;
    }
    if t.ends_with('\\') {
        return false;
    }
    let st = scan(t);
    if st.in_string || st.depth > 0 {
        return false;
    }
    if CONTINUATIONS.iter().any(|c| t.ends_with(c)) {
        return false;
    }
    if t.ends_with(';') {
        return true;
    }
    // system operations and single-line option statements: `::relations`, `:create r {…}`
    if t.starts_with(':') {
        return true;
    }
    // blocks `{ … }` and scripts that contain an entry rule `?[…] := …`
    t.starts_with('{') || t.lines().any(|l| l.trim_start().starts_with('?'))
}

#[cfg(test)]
mod tests {
    use super::is_complete as c;

    #[test]
    fn single_line() {
        assert!(c("?[x] := x = 1"));
        assert!(c("::relations"));
        assert!(c(".help"));
        assert!(c(":create r {a: Int}"));
    }

    #[test]
    fn needs_more() {
        assert!(!c("?[x] := *route{fr: 'FRA',"));
        assert!(!c("?[x] := x = 'abc"));
        assert!(!c("?[x] :="));
        assert!(!c("reachable[to] := *route{fr: 'FRA', to}"));
        assert!(!c("?[x] := x = 1 \\"));
    }

    #[test]
    fn blank_line_submits() {
        assert!(c("reachable[to] := *route{fr: 'FRA', to}\n"));
    }

    #[test]
    fn brackets_in_strings_and_comments() {
        assert!(c("?[x] := x = '(' # {"));
    }
}
