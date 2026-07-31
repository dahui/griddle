//! Text KeyValues (KV1) — the format of `libraryfolders.vdf`, `appmanifest_*.acf`,
//! `loginusers.vdf` and `localconfig.vdf`.
//!
//! ```text
//! "AppState"
//! {
//!     "appid"      "228980"
//!     "name"       "Steamworks Common Redistributables"
//!     "UserConfig" { }
//! }
//! ```
//!
//! # Read-only, and that shapes the design
//!
//! We never write text VDF — the only files this project writes are `shortcuts.vdf` (binary),
//! artwork, and our own settings. So values are `String` here, unlike [`super::binary`] where
//! byte-exact round-tripping forced raw `Vec<u8>`. If that ever changes, this decision has to
//! be revisited before adding a writer.
//!
//! # The defensiveness that matters
//!
//! `libraryfolders.vdf` numbers its children `"0"`, `"1"`, … but some client versions emit a
//! **scalar** sibling among them (`"contentstatsid" "7785519366728146050"`). Code that assumes
//! every child of `libraryfolders` is a map will panic or silently skip real libraries — this
//! is the single most common breakage in third-party parsers.
//! `[VERIFIED-SOURCE — steamlocate-rs #3, HXE #218]`
//!
//! The parser keeps both shapes; [`Value::as_map`] returns `None` for a scalar, so callers
//! filter rather than crash. See `steam::library`.

use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub enum Value {
    Map(Vec<Entry>),
    Str(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Document {
    pub entries: Vec<Entry>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("unexpected end of input at byte {offset} (expected {expected})")]
    UnexpectedEof {
        offset: usize,
        expected: &'static str,
    },

    #[error("unterminated quoted string starting at byte {offset}")]
    UnterminatedString { offset: usize },

    #[error("unexpected '}}' at byte {offset}")]
    UnexpectedClose { offset: usize },

    #[error("unclosed '{{' opened at byte {offset}")]
    UnclosedBrace { offset: usize },

    #[error("nesting deeper than {limit} at byte {offset}")]
    TooDeep { offset: usize, limit: usize },
}

/// Real files nest a handful deep; this only exists so a malformed file cannot drive the
/// recursive parser into a stack overflow.
const MAX_DEPTH: usize = 64;

pub fn parse(input: &str) -> Result<Document, Error> {
    let mut p = Parser {
        s: input.as_bytes(),
        pos: 0,
    };
    let entries = p.parse_entries(0, true)?;
    Ok(Document { entries })
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_entries(&mut self, depth: usize, top: bool) -> Result<Vec<Entry>, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TooDeep {
                offset: self.pos,
                limit: MAX_DEPTH,
            });
        }
        let mut entries = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                None => {
                    if top {
                        return Ok(entries);
                    }
                    return Err(Error::UnexpectedEof {
                        offset: self.pos,
                        expected: "'}'",
                    });
                }
                Some(b'}') => {
                    if top {
                        return Err(Error::UnexpectedClose { offset: self.pos });
                    }
                    self.pos += 1;
                    return Ok(entries);
                }
                _ => {}
            }

            let key = self.read_token()?;
            self.skip_trivia();

            let value = match self.peek() {
                Some(b'{') => {
                    let open = self.pos;
                    self.pos += 1;
                    let children = self.parse_entries(depth + 1, false)?;
                    if self.pos > self.s.len() {
                        return Err(Error::UnclosedBrace { offset: open });
                    }
                    Value::Map(children)
                }
                Some(_) => Value::Str(self.read_token()?),
                None => {
                    return Err(Error::UnexpectedEof {
                        offset: self.pos,
                        expected: "a value",
                    });
                }
            };

            // Platform conditionals like `"key" "value" [$WIN32]` follow the value. We do not
            // evaluate them — no file we read uses them meaningfully — but they must not be
            // mistaken for the next key.
            self.skip_conditional();

            entries.push(Entry { key, value });
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    /// Whitespace and `//` comments.
    fn skip_trivia(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.pos += 1;
            }
            if self.peek() == Some(b'/') && self.s.get(self.pos + 1) == Some(&b'/') {
                while !matches!(self.peek(), None | Some(b'\n')) {
                    self.pos += 1;
                }
                continue;
            }
            return;
        }
    }

    fn skip_conditional(&mut self) {
        let save = self.pos;
        self.skip_trivia();
        if self.peek() == Some(b'[') {
            while !matches!(self.peek(), None | Some(b']')) {
                self.pos += 1;
            }
            if self.peek() == Some(b']') {
                self.pos += 1;
            }
        } else {
            self.pos = save;
        }
    }

    /// A quoted string (with escapes) or a bare token.
    fn read_token(&mut self) -> Result<String, Error> {
        if self.peek() == Some(b'"') {
            let start = self.pos;
            self.pos += 1;
            let mut out = String::new();
            loop {
                match self.peek() {
                    None => return Err(Error::UnterminatedString { offset: start }),
                    Some(b'"') => {
                        self.pos += 1;
                        return Ok(out);
                    }
                    Some(b'\\') => {
                        self.pos += 1;
                        let c = self
                            .peek()
                            .ok_or(Error::UnterminatedString { offset: start })?;
                        self.pos += 1;
                        out.push(match c {
                            b'n' => '\n',
                            b't' => '\t',
                            b'r' => '\r',
                            // `\\` and `\"` are the ones that matter: Windows paths in these
                            // files are escaped as `C:\\Program Files (x86)\\Steam`.
                            other => other as char,
                        });
                    }
                    Some(_) => {
                        let s = self.pos;
                        while !matches!(self.peek(), None | Some(b'"') | Some(b'\\')) {
                            self.pos += 1;
                        }
                        out.push_str(&String::from_utf8_lossy(&self.s[s..self.pos]));
                    }
                }
            }
        }

        let start = self.pos;
        while !matches!(
            self.peek(),
            None | Some(b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'{' | b'}')
        ) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(Error::UnexpectedEof {
                offset: self.pos,
                expected: "a token",
            });
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.pos]).into_owned())
    }
}

impl Value {
    /// The nested entries — `None` for a scalar. **Callers must handle `None`**: see the
    /// module docs on `contentstatsid`.
    pub fn as_map(&self) -> Option<&[Entry]> {
        match self {
            Value::Map(e) => Some(e),
            Value::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Map(_) => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_str()?.trim().parse().ok()
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.as_str()?.trim().parse().ok()
    }
}

/// Find a value by key, case-insensitively.
///
/// Steam is inconsistent about casing across files and versions (`AppName` vs `appname` in
/// shortcuts; `StateFlags` is stable but `appid` is lowercase while its siblings are not), so
/// exact matching is a latent bug.
pub fn get<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|e| e.key.eq_ignore_ascii_case(key))
        .map(|e| &e.value)
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Map(e) => f
                .debug_map()
                .entries(e.iter().map(|x| (&x.key, &x.value)))
                .finish(),
            Value::Str(s) => write!(f, "{s:?}"),
        }
    }
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} => {:?}", self.key, self.value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn parses_an_appmanifest() {
        // Shape and values taken from the real appmanifest_228980.acf on this machine.
        let src = r#"
"AppState"
{
	"appid"		"228980"
	"Universe"		"1"
	"LauncherPath"		"C:\\Program Files (x86)\\Steam\\steam.exe"
	"name"		"Steamworks Common Redistributables"
	"StateFlags"		"4"
	"installdir"		"Steamworks Shared"
	"SizeOnDisk"		"491869131"
	"UserConfig"
	{
	}
	"MountedConfig"
	{
		"language"		"english"
	}
}
"#;
        let doc = parse(src).unwrap();
        let app = get(&doc.entries, "AppState").unwrap().as_map().unwrap();
        assert_eq!(get(app, "appid").unwrap().as_u32(), Some(228980));
        assert_eq!(get(app, "StateFlags").unwrap().as_u32(), Some(4));
        assert_eq!(
            get(app, "installdir").unwrap().as_str(),
            Some("Steamworks Shared")
        );
        // Escaped backslashes must collapse to single ones.
        assert_eq!(
            get(app, "LauncherPath").unwrap().as_str(),
            Some(r"C:\Program Files (x86)\Steam\steam.exe")
        );
        // An empty map is a map, not a string.
        assert_eq!(get(app, "UserConfig").unwrap().as_map().unwrap().len(), 0);
        assert_eq!(
            get(app, "MountedConfig").unwrap().as_map().unwrap().len(),
            1
        );
    }

    #[test]
    fn case_insensitive_lookup() {
        let doc = parse(r#""AppState" { "AppID" "7" }"#).unwrap();
        let app = get(&doc.entries, "appstate").unwrap().as_map().unwrap();
        assert_eq!(get(app, "appid").unwrap().as_u32(), Some(7));
    }

    /// The scalar-among-numbered-keys case that breaks naive parsers.
    #[test]
    fn tolerates_a_scalar_sibling_among_numbered_library_entries() {
        let src = r#"
"libraryfolders"
{
	"contentstatsid"		"7785519366728146050"
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"491869131"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
		}
	}
}
"#;
        let doc = parse(src).unwrap();
        let lf = get(&doc.entries, "libraryfolders")
            .unwrap()
            .as_map()
            .unwrap();
        assert_eq!(
            lf.len(),
            3,
            "the scalar sibling must be preserved, not dropped"
        );

        // The consumer pattern: skip children that are not maps.
        let paths: Vec<&str> = lf
            .iter()
            .filter_map(|e| e.value.as_map())
            .filter_map(|m| get(m, "path")?.as_str())
            .collect();
        assert_eq!(paths, [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);

        // And the scalar is still readable if anyone wants it.
        assert_eq!(
            get(lf, "contentstatsid").unwrap().as_str(),
            Some("7785519366728146050")
        );
    }

    #[test]
    fn reads_the_nested_apps_map() {
        let src = r#""libraryfolders" { "0" { "apps" { "220" "1234" "440" "5678" } } }"#;
        let doc = parse(src).unwrap();
        let lf = get(&doc.entries, "libraryfolders")
            .unwrap()
            .as_map()
            .unwrap();
        let zero = get(lf, "0").unwrap().as_map().unwrap();
        let apps = get(zero, "apps").unwrap().as_map().unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(get(apps, "440").unwrap().as_u64(), Some(5678));
    }

    #[test]
    fn skips_comments() {
        let src = r#"
// leading comment
"root"   // trailing comment
{
    "a" "1"   // another
    // whole line
    "b" "2"
}
"#;
        let doc = parse(src).unwrap();
        let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(get(root, "b").unwrap().as_str(), Some("2"));
    }

    #[test]
    fn handles_platform_conditionals() {
        let doc = parse(r#""root" { "a" "1" [$WIN32] "b" "2" }"#).unwrap();
        let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
        assert_eq!(root.len(), 2, "the conditional must not be read as a key");
        assert_eq!(get(root, "b").unwrap().as_str(), Some("2"));
    }

    #[test]
    fn handles_unquoted_tokens() {
        let doc = parse("root { key value }").unwrap();
        let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
        assert_eq!(get(root, "key").unwrap().as_str(), Some("value"));
    }

    #[test]
    fn preserves_duplicate_keys_and_order() {
        // Order matters for `libraryfolders`; duplicates are legal in KV1.
        let doc = parse(r#""r" { "k" "1" "k" "2" }"#).unwrap();
        let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].value.as_str(), Some("1"));
        assert_eq!(r[1].value.as_str(), Some("2"));
        // `get` returns the first, matching Steam's own behaviour.
        assert_eq!(get(r, "k").unwrap().as_str(), Some("1"));
    }

    #[test]
    fn escapes_inside_quoted_strings() {
        let doc = parse(r#""r" { "s" "a\"b\\c\nd" }"#).unwrap();
        let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
        assert_eq!(get(r, "s").unwrap().as_str(), Some("a\"b\\c\nd"));
    }

    #[test]
    fn rejects_unterminated_string() {
        assert!(matches!(
            parse(r#""r" { "k" "unterminated "#),
            Err(Error::UnterminatedString { .. })
        ));
    }

    #[test]
    fn rejects_unbalanced_close() {
        assert!(matches!(
            parse(r#""r" { "k" "v" } }"#),
            Err(Error::UnexpectedClose { .. })
        ));
    }

    #[test]
    fn rejects_missing_close() {
        assert!(matches!(
            parse(r#""r" { "k" "v" "#),
            Err(Error::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn empty_input_is_an_empty_document() {
        assert_eq!(parse("").unwrap().entries.len(), 0);
        assert_eq!(parse("   \n // just a comment\n").unwrap().entries.len(), 0);
    }

    #[test]
    fn handles_non_utf8_gracefully() {
        // Steam writes game names in whatever encoding it has; a lossy read must not panic.
        let raw = b"\"r\" { \"name\" \"Street Fighter\xe2\x84\xa2 6\" }";
        let doc = parse(&String::from_utf8_lossy(raw)).unwrap();
        let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
        assert_eq!(get(r, "name").unwrap().as_str(), Some("Street Fighter™ 6"));
    }
}
