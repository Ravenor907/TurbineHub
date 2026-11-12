//! Valve's text KeyValues format ("VDF"), as used by libraryfolders.vdf and appmanifest_*.acf, and its binary form,
//! as used by shortcuts.vdf (the player's non-Steam games).
//!
//! Keys are case-insensitive in Steam's own reader, so lookups here are too.

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

impl Vdf {
    /// The child with this key (case-insensitive), if this is an object.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    /// Walks a path of keys: `v.path(&["AppState", "installdir"])`.
    pub fn path(&self, keys: &[&str]) -> Option<&Vdf> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    pub fn str_at(&self, keys: &[&str]) -> Option<&str> {
        self.path(keys).and_then(Vdf::as_str)
    }

    /// The (key, value) pairs of an object, in file order.
    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(items) => items,
            Vdf::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct VdfError {
    pub line: usize,
    pub message: &'static str,
}

impl std::fmt::Display for VdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for VdfError {}

/// Parses a whole file. The result is an object holding the top-level keys.
pub fn parse(text: &str) -> Result<Vdf, VdfError> {
    let mut p = Parser { chars: text.chars().collect(), pos: 0, line: 1 };
    let items = p.object_body(0)?;
    Ok(Vdf::Obj(items))
}

/// Most levels of nested objects. Real Steam files use a handful; the limit keeps a hostile file from exhausting the
/// stack.
pub const MAX_DEPTH: usize = 128;

struct Parser {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

enum Tok {
    Str(String),
    Open,
    Close,
    End,
}

impl Parser {
    fn err(&self, message: &'static str) -> VdfError {
        VdfError { line: self.line, message }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if let Some(ch) = c {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
            }
        }
        c
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() || c == '\u{feff}' => {
                    self.bump();
                }
                Some('/') if self.chars.get(self.pos + 1) == Some(&'/') => {
                    while let Some(c) = self.bump() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn token(&mut self) -> Result<Tok, VdfError> {
        self.skip_space_and_comments();
        match self.peek() {
            None => Ok(Tok::End),
            Some('{') => {
                self.bump();
                Ok(Tok::Open)
            }
            Some('}') => {
                self.bump();
                Ok(Tok::Close)
            }
            Some('"') => {
                self.bump();
                let mut s = String::new();
                loop {
                    match self.bump() {
                        None => return Err(self.err("unterminated string")),
                        Some('"') => break,
                        Some('\\') => match self.bump() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('\\') => s.push('\\'),
                            Some('"') => s.push('"'),
                            Some(other) => {
                                s.push('\\');
                                s.push(other);
                            }
                            None => return Err(self.err("unterminated string")),
                        },
                        Some(c) => s.push(c),
                    }
                }
                // Conditionals like [$WIN32] after a value are skipped.
                self.skip_conditional();
                Ok(Tok::Str(s))
            }
            Some(_) => {
                // Unquoted token: runs to whitespace or a brace.
                let mut s = String::new();
                while let Some(c) = self.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    self.bump();
                }
                self.skip_conditional();
                Ok(Tok::Str(s))
            }
        }
    }

    fn skip_conditional(&mut self) {
        let save = (self.pos, self.line);
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.bump();
            } else {
                break;
            }
        }
        if self.peek() == Some('[') {
            while let Some(c) = self.bump() {
                if c == ']' || c == '\n' {
                    return;
                }
            }
        } else {
            self.pos = save.0;
            self.line = save.1;
        }
    }

    /// Reads the keys of an object. `depth` is 0 for the top level and counts the braces around it.
    fn object_body(&mut self, depth: usize) -> Result<Vec<(String, Vdf)>, VdfError> {
        let nested = depth > 0;
        let mut items = Vec::new();
        loop {
            let key = match self.token()? {
                Tok::Str(s) => s,
                Tok::Close if nested => return Ok(items),
                Tok::Close => return Err(self.err("unexpected '}'")),
                Tok::End if !nested => return Ok(items),
                Tok::End => return Err(self.err("missing '}'")),
                Tok::Open => return Err(self.err("expected a key, found '{'")),
            };
            let value = match self.token()? {
                Tok::Str(s) => Vdf::Str(s),
                Tok::Open if depth >= MAX_DEPTH => return Err(self.err("nested too deeply")),
                Tok::Open => Vdf::Obj(self.object_body(depth + 1)?),
                Tok::Close | Tok::End => return Err(self.err("key without a value")),
            };
            items.push((key, value));
        }
    }
}

// ---------------------------------------------------------------------------------------------------- binary

/// Binary VDF type bytes.
const B_OBJ: u8 = 0x00;
const B_STR: u8 = 0x01;
const B_INT: u8 = 0x02;
const B_FLOAT: u8 = 0x03;
const B_PTR: u8 = 0x04;
const B_WSTR: u8 = 0x05;
const B_COLOR: u8 = 0x06;
const B_U64: u8 = 0x07;
const B_END: u8 = 0x08;
const B_I64: u8 = 0x0A;
const B_END_ALT: u8 = 0x0B;

/// Parses binary VDF (shortcuts.vdf). Each item is a type byte, a key ending in a zero byte, then the value; an
/// object ends with 0x08. Strings become `Vdf::Str`; 32-bit numbers become their unsigned decimal form (Steam keeps
/// a shortcut's appid as a 32-bit number whose top bit is set, so "2147483649", not "-2147483647"); 64-bit numbers
/// and floats become decimal too. Never panics on a broken file; `line` in an error is the byte offset.
pub fn parse_binary(bytes: &[u8]) -> Result<Vdf, VdfError> {
    let mut r = BinReader { b: bytes, pos: 0 };
    let items = r.object(0, true)?;
    Ok(Vdf::Obj(items))
}

struct BinReader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl BinReader<'_> {
    fn err(&self, message: &'static str) -> VdfError {
        VdfError { line: self.pos, message }
    }

    fn byte(&mut self) -> Option<u8> {
        let v = *self.b.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], VdfError> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| self.err("file ends inside a value"))?;
        let out = &self.b[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn cstr(&mut self) -> Result<String, VdfError> {
        let rest = &self.b[self.pos..];
        let len = rest.iter().position(|&c| c == 0).ok_or_else(|| self.err("text without an end"))?;
        let s = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Ok(s)
    }

    fn wstr(&mut self) -> Result<String, VdfError> {
        let mut units = Vec::new();
        loop {
            let b = self.take(2)?;
            let u = u16::from_le_bytes([b[0], b[1]]);
            if u == 0 {
                return Ok(String::from_utf16_lossy(&units));
            }
            units.push(u);
        }
    }

    /// Reads items until the object's end byte (or, at the top level, the end of the file).
    fn object(&mut self, depth: usize, top: bool) -> Result<Vec<(String, Vdf)>, VdfError> {
        let mut items = Vec::new();
        loop {
            let Some(t) = self.byte() else {
                return if top { Ok(items) } else { Err(self.err("missing end of object")) };
            };
            if t == B_END || t == B_END_ALT {
                // At the top level this is the end byte Steam writes after the root object.
                return Ok(items);
            }
            let key = self.cstr()?;
            let value = match t {
                B_OBJ if depth >= MAX_DEPTH => return Err(self.err("nested too deeply")),
                B_OBJ => Vdf::Obj(self.object(depth + 1, false)?),
                B_STR => Vdf::Str(self.cstr()?),
                B_WSTR => Vdf::Str(self.wstr()?),
                B_INT | B_PTR | B_COLOR => {
                    let b = self.take(4)?;
                    Vdf::Str(u32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string())
                }
                B_FLOAT => {
                    let b = self.take(4)?;
                    Vdf::Str(f32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string())
                }
                B_U64 => {
                    let b = self.take(8)?;
                    Vdf::Str(u64::from_le_bytes(b.try_into().unwrap_or([0; 8])).to_string())
                }
                B_I64 => {
                    let b = self.take(8)?;
                    Vdf::Str(i64::from_le_bytes(b.try_into().unwrap_or([0; 8])).to_string())
                }
                _ => return Err(self.err("unknown value type")),
            };
            items.push((key, value));
        }
    }
}

// ---- Binary VDF, kept exactly (for writing shortcuts.vdf) ----
//
// `parse_binary` turns every value into text, which is right for reading but would lose each value's type on the
// way back. To change shortcuts.vdf, Turbine Hub keeps each item's type byte and its value's exact bytes, changes only
// the items it adds or removes, and writes everything else back byte for byte.

/// One item of a binary VDF object: its type byte, its key (the bytes before the zero) and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinItem {
    pub kind: u8,
    pub key: Vec<u8>,
    pub value: BinValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinValue {
    /// The items, then the end byte the file used (0x08, or 0x0B in some files).
    Obj(Vec<BinItem>, u8),
    /// The value's bytes exactly as in the file (a text's closing zero included).
    Raw(Vec<u8>),
}

/// A whole binary VDF file: the top-level items, the end byte after them (when the file has one) and anything after
/// that, all kept as they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinDoc {
    pub items: Vec<BinItem>,
    pub end: Option<u8>,
    pub tail: Vec<u8>,
}

impl BinItem {
    /// A text value (type 0x01).
    pub fn text(key: &str, value: &str) -> BinItem {
        let mut v = value.as_bytes().to_vec();
        v.retain(|b| *b != 0);
        v.push(0);
        BinItem { kind: B_STR, key: key.as_bytes().to_vec(), value: BinValue::Raw(v) }
    }

    /// A 32-bit number (type 0x02), little endian.
    pub fn int(key: &str, value: u32) -> BinItem {
        BinItem { kind: B_INT, key: key.as_bytes().to_vec(), value: BinValue::Raw(value.to_le_bytes().to_vec()) }
    }

    /// An object (type 0x00) ending with 0x08.
    pub fn obj(key: &str, items: Vec<BinItem>) -> BinItem {
        BinItem { kind: B_OBJ, key: key.as_bytes().to_vec(), value: BinValue::Obj(items, B_END) }
    }

    pub fn key_is(&self, key: &str) -> bool {
        self.key.eq_ignore_ascii_case(key.as_bytes())
    }

    /// The text of a text value (type 0x01), without its closing zero.
    pub fn as_text(&self) -> Option<String> {
        match (&self.value, self.kind) {
            (BinValue::Raw(b), B_STR) => Some(String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(b)).into_owned()),
            _ => None,
        }
    }

    /// A 32-bit number (type 0x02).
    pub fn as_int(&self) -> Option<u32> {
        match (&self.value, self.kind) {
            (BinValue::Raw(b), B_INT) if b.len() == 4 => Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => None,
        }
    }

    pub fn children(&self) -> &[BinItem] {
        match &self.value {
            BinValue::Obj(items, _) => items,
            BinValue::Raw(_) => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<BinItem>> {
        match &mut self.value {
            BinValue::Obj(items, _) => Some(items),
            BinValue::Raw(_) => None,
        }
    }

    /// The child with this key (case-insensitive).
    pub fn child(&self, key: &str) -> Option<&BinItem> {
        self.children().iter().find(|i| i.key_is(key))
    }
}

/// Parses binary VDF keeping every byte (see `BinDoc`). Never panics on a broken file.
pub fn parse_binary_doc(bytes: &[u8]) -> Result<BinDoc, VdfError> {
    let mut r = BinReader { b: bytes, pos: 0 };
    let (items, end) = r.raw_object(0)?;
    let tail = bytes[r.pos..].to_vec();
    Ok(BinDoc { items, end, tail })
}

/// The file's bytes. `write_binary_doc(&parse_binary_doc(b)?) == b` for every file it parses.
pub fn write_binary_doc(doc: &BinDoc) -> Vec<u8> {
    fn items(out: &mut Vec<u8>, list: &[BinItem]) {
        for i in list {
            out.push(i.kind);
            out.extend_from_slice(&i.key);
            out.push(0);
            match &i.value {
                BinValue::Obj(children, end) => {
                    items(out, children);
                    out.push(*end);
                }
                BinValue::Raw(b) => out.extend_from_slice(b),
            }
        }
    }
    let mut out = Vec::new();
    items(&mut out, &doc.items);
    if let Some(e) = doc.end {
        out.push(e);
    }
    out.extend_from_slice(&doc.tail);
    out
}

impl BinReader<'_> {
    /// Items up to the object's end byte (None at the top level when the file just ends).
    fn raw_object(&mut self, depth: usize) -> Result<(Vec<BinItem>, Option<u8>), VdfError> {
        let mut items = Vec::new();
        loop {
            let Some(t) = self.byte() else {
                return if depth == 0 { Ok((items, None)) } else { Err(self.err("missing end of object")) };
            };
            if t == B_END || t == B_END_ALT {
                return Ok((items, Some(t)));
            }
            let rest = &self.b[self.pos..];
            let len = rest.iter().position(|&c| c == 0).ok_or_else(|| self.err("text without an end"))?;
            let key = rest[..len].to_vec();
            self.pos += len + 1;
            let start = self.pos;
            let value = match t {
                B_OBJ if depth >= MAX_DEPTH => return Err(self.err("nested too deeply")),
                B_OBJ => {
                    let (children, end) = self.raw_object(depth + 1)?;
                    BinValue::Obj(children, end.unwrap_or(B_END))
                }
                B_STR => {
                    self.cstr()?;
                    BinValue::Raw(self.b[start..self.pos].to_vec())
                }
                B_WSTR => {
                    self.wstr()?;
                    BinValue::Raw(self.b[start..self.pos].to_vec())
                }
                B_INT | B_PTR | B_COLOR | B_FLOAT => BinValue::Raw(self.take(4)?.to_vec()),
                B_U64 | B_I64 => BinValue::Raw(self.take(8)?.to_vec()),
                _ => return Err(self.err("unknown value type")),
            };
            items.push(BinItem { kind: t, key, value });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARY: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/deck/.local/share/Steam"
		"label"		""
		"apps"
		{
			"228980"		"0"
			"489830"		"14958233"
		}
	}
	// SD card
	"1"
	{
		"path"		"/run/media/deck/SD \"Card\""
		"apps"
		{
			"892970"		"1234"
		}
	}
}
"#;

    #[test]
    fn reads_library_folders() {
        let v = parse(LIBRARY).unwrap();
        let lf = v.get("LibraryFolders").unwrap();
        assert_eq!(lf.entries().len(), 2);
        assert_eq!(lf.str_at(&["0", "path"]), Some("/home/deck/.local/share/Steam"));
        assert_eq!(lf.str_at(&["1", "path"]), Some("/run/media/deck/SD \"Card\""));
        let apps: Vec<&str> = lf.path(&["0", "apps"]).unwrap().entries().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(apps, ["228980", "489830"]);
    }

    #[test]
    fn skips_conditionals_and_unquoted() {
        let v = parse("root { key value [$WIN32] other \"x\" }").unwrap();
        assert_eq!(v.str_at(&["root", "key"]), Some("value"));
        assert_eq!(v.str_at(&["root", "other"]), Some("x"));
    }

    #[test]
    fn reports_broken_files() {
        assert!(parse("\"a\" { \"b\" \"c\"").is_err());
        assert!(parse("\"a\"").is_err());
        assert!(parse("}").is_err());
    }

    #[test]
    fn nesting_limit_is_exact() {
        let at = |n: usize| format!("{}\"k\" \"v\"{}", "\"a\" {".repeat(n), "}".repeat(n));
        assert!(parse(&at(MAX_DEPTH)).is_ok());
        assert_eq!(parse(&at(MAX_DEPTH + 1)).unwrap_err().message, "nested too deeply");
    }
}
