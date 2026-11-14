/// Where the database lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Embedded { engine: String, path: String },
    Remote { url: String },
}

const ENGINES: [&str; 3] = ["mem", "sqlite", "rocksdb"];

impl Target {
    pub fn parse(s: &str) -> Target {
        if s.starts_with("http://") || s.starts_with("https://") {
            return Target::Remote { url: s.to_string() };
        }
        if let Some((engine, path)) = s.split_once(':') {
            if ENGINES.contains(&engine) {
                return Target::Embedded { engine: engine.into(), path: path.into() };
            }
        }
        if s == "mem" {
            return Target::Embedded { engine: "mem".into(), path: String::new() };
        }
        // A bare path: assume the default persistent engine, by extension.
        let engine = if s.ends_with(".db") || s.ends_with(".sqlite") || s.ends_with(".sqlite3") {
            "sqlite"
        } else {
            "rocksdb"
        };
        Target::Embedded { engine: engine.into(), path: s.into() }
    }

    pub fn describe(&self) -> String {
        match self {
            Target::Embedded { engine, path } if path.is_empty() => format!("{engine}:"),
            Target::Embedded { engine, path } => format!("{engine}:{path}"),
            Target::Remote { url } => url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uris() {
        assert_eq!(Target::parse("mem:"), Target::Embedded { engine: "mem".into(), path: "".into() });
        assert_eq!(Target::parse("sqlite:a.db"), Target::Embedded { engine: "sqlite".into(), path: "a.db".into() });
        assert_eq!(Target::parse("rocksdb:./d"), Target::Embedded { engine: "rocksdb".into(), path: "./d".into() });
        assert_eq!(Target::parse("data.db"), Target::Embedded { engine: "sqlite".into(), path: "data.db".into() });
        assert_eq!(Target::parse("./data"), Target::Embedded { engine: "rocksdb".into(), path: "./data".into() });
        assert_eq!(Target::parse("http://x:9070"), Target::Remote { url: "http://x:9070".into() });
    }
}
