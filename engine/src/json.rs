//! A small, strict JSON reader for the launcher files Turbine Hub reads (Heroic's installed lists and similar).
//! Reading only: Turbine Hub never rewrites another program's JSON.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn str_at(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Json::as_str)
    }

    pub fn as_array(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => &[],
        }
    }

    pub fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(items) => items,
            _ => &[],
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// True for what JavaScript counts as true: anything but null, false, 0, NaN and "".
    pub fn truthy(&self) -> bool {
        match self {
            Json::Null => false,
            Json::Bool(b) => *b,
            Json::Num(n) => *n != 0.0 && !n.is_nan(),
            Json::Str(s) => !s.is_empty(),
            Json::Arr(_) | Json::Obj(_) => true,
        }
    }

    /// The value written back as JSON text. Whole numbers are written without a fraction.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => out.push_str(&number_text(*n)),
            Json::Str(s) => out.push_str(&quote(s)),
            Json::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(items) => {
                out.push('{');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&quote(k));
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// A number as JSON writes it: whole numbers without a fraction, and null for what JSON can't hold.
fn number_text(n: f64) -> String {
    if !n.is_finite() {
        "null".into()
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[derive(Debug, PartialEq)]
pub struct JsonError {
    pub offset: usize,
    pub message: &'static str,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

/// Most levels of nested arrays and objects: deep enough for any real launcher file, shallow enough that a hostile
/// file can't exhaust the stack.
pub const MAX_DEPTH: usize = 128;

pub fn parse(text: &str) -> Result<Json, JsonError> {
    let mut p = P { b: text.as_bytes(), i: 0 };
    if p.b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.i = 3;
    }
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(p.err("extra text after the value"));
    }
    Ok(v)
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn err(&self, message: &'static str) -> JsonError {
        JsonError { offset: self.i, message }
    }

    fn ws(&mut self) {
        while let Some(&c) = self.b.get(self.i) {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn lit(&mut self, word: &[u8], v: Json) -> Result<Json, JsonError> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("unexpected word"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.ws();
        if depth >= MAX_DEPTH && matches!(self.b.get(self.i), Some(b'[' | b'{')) {
            return Err(self.err("nested too deeply"));
        }
        match self.b.get(self.i) {
            None => Err(self.err("unexpected end")),
            Some(b'n') => self.lit(b"null", Json::Null),
            Some(b't') => self.lit(b"true", Json::Bool(true)),
            Some(b'f') => self.lit(b"false", Json::Bool(false)),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(items));
                        }
                        _ => return Err(self.err("expected ',' or ']'")),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(items));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return Err(self.err("expected a key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return Err(self.err("expected ':'"));
                    }
                    self.i += 1;
                    let v = self.value(depth + 1)?;
                    items.push((k, v));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(items));
                        }
                        _ => return Err(self.err("expected ',' or '}'")),
                    }
                }
            }
            Some(c) if *c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        self.i - start
    }

    /// The JSON number grammar exactly: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                if self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    return Err(self.err("bad number (leading zero)"));
                }
            }
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.err("bad number")),
        }
        if self.b.get(self.i) == Some(&b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return Err(self.err("bad number (no digits after '.')"));
            }
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return Err(self.err("bad number (no digits in the exponent)"));
            }
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| self.err("bad number"))?;
        s.parse::<f64>().map(Json::Num).map_err(|_| self.err("bad number"))
    }

    /// Exactly four hex digits (no sign, no spaces).
    fn hex4(&mut self) -> Result<u32, JsonError> {
        let s = self.b.get(self.i..self.i + 4).ok_or_else(|| self.err("short \\u escape"))?;
        let mut v = 0u32;
        for &c in s {
            let d = (c as char).to_digit(16).ok_or_else(|| self.err("bad \\u escape"))?;
            v = v * 16 + d;
        }
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let start = self.i;
            while let Some(&c) = self.b.get(self.i) {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.b[start..self.i]).map_err(|_| self.err("text is not UTF-8"))?);
            match self.b.get(self.i) {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let e = *self.b.get(self.i).ok_or_else(|| self.err("unterminated string"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                if self.b.get(self.i..self.i + 2) != Some(b"\\u") {
                                    return Err(self.err("lone surrogate"));
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("lone surrogate"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(code).ok_or_else(|| self.err("bad \\u escape"))?);
                        }
                        _ => return Err(self.err("bad escape")),
                    }
                }
                Some(_) => return Err(self.err("control character in string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_values() {
        let v = parse(r#"{"a": [1, -2.5e1, true, null], "b": "x\"y\u00e9\ud83d\ude00", "c": {}}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_array().len(), 4);
        assert_eq!(v.str_at("b"), Some("x\"y\u{e9}\u{1F600}"));
        assert_eq!(v.get("c"), Some(&Json::Obj(vec![])));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in ["", "{", "[1,]", "{\"a\" 1}", "\"\\ud800\"", "nul", "1 2", "\"a\nb\""] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = "[".repeat(1000);
        assert!(parse(&deep).is_err());
        for bad in ["01", "-", "-a", "1.", ".5", "1e", "1e+", "+1", "\"\\u+041\"", "\"\\u 041\"", "\"\\u004\""] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
        for good in ["0", "-0", "0.5", "-1.25e-3", "1E+9", "10", "\"\\u00Ff\""] {
            assert!(parse(good).is_ok(), "{good:?} should parse");
        }
    }

    #[test]
    fn nesting_limit_is_exact() {
        let at = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(&at(MAX_DEPTH)).is_ok());
        assert!(parse(&at(MAX_DEPTH + 1)).is_err());
        let objs = |n: usize| format!("{}1{}", "{\"a\":".repeat(n), "}".repeat(n));
        assert!(parse(&objs(MAX_DEPTH)).is_ok());
        assert!(parse(&objs(MAX_DEPTH + 1)).is_err());
    }
}

/// Writes `s` as a JSON string, quotes included.
pub fn quote(s: &str) -> String {
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

#[cfg(test)]
mod quote_tests {
    #[test]
    fn round_trips() {
        let s = "a\"b\\c\nd\u{1}é";
        assert_eq!(super::parse(&super::quote(s)).unwrap().as_str(), Some(s));
    }

    #[test]
    fn writes_values_back() {
        let text = r#"{"a":[1,2.5,-3,true,null],"b":{"c":"d\"e"},"f":[]}"#;
        assert_eq!(super::parse(text).unwrap().to_text(), text);
        assert_eq!(super::Json::Num(1e20).to_text(), "100000000000000000000");
        assert_eq!(super::Json::Num(f64::NAN).to_text(), "null");
    }

    #[test]
    fn truthy_follows_javascript() {
        use super::Json;
        for (v, t) in [(Json::Null, false), (Json::Bool(false), false), (Json::Num(0.0), false), (Json::Str(String::new()), false), (Json::Num(2.0), true), (Json::Str("x".into()), true), (Json::Arr(vec![]), true)] {
            assert_eq!(v.truthy(), t, "{v:?}");
        }
    }
}
