// SPDX-License-Identifier: MIT
//! Reads only the fields the menus need out of Arena's full composition JSON (swagger.yaml
//! schemas Composition, Layer, Column, Clip) with a single forward byte scan over the message, in
//! place: no copy of the 18-29 MB document, no JSON tree. Everything outside the wanted paths is
//! skipped byte-wise; the allocations are the snapshot itself.
//!
//! ```text
//! { "name":    {"value": <string>}                                   → composition name
//!   "decks":   [ {"name":{"value":…}, "selected":{"value":<bool>}} ] → first selected deck
//!   "layers":  [ {"id", "name":{"value","id"},
//!                 "clips":[ {"id", "name":{"value"}, "connected":{"value"}} ]} ]
//!   "columns": [ {"id", "name":{"value","id"}} ] }
//! ```

use crate::error::ArenaError;
use crate::snapshot::{ColumnInfo, CompositionSnapshot, LayerInfo, SlotInfo};

/// True for a websocket message that carries a full composition (no `type`; full compositions
/// start with the composition name parameter).
pub fn looks_like_composition(bytes: &[u8]) -> bool {
    bytes.starts_with(b"{\"name\":")
}

/// The snapshot in `bytes`, or `BadResponse("composition JSON: <why> at byte <n>")`.
pub fn decode(bytes: &[u8]) -> Result<CompositionSnapshot, ArenaError> {
    let mut s = Scanner { p: bytes, i: 0 };
    s.composition().map_err(|e| ArenaError::BadResponse(format!("composition JSON: {} at byte {}", e.message, e.offset)))
}

#[derive(Debug)]
struct Failure {
    message: String,
    offset: usize,
}

type R<T> = Result<T, Failure>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Name,
    Decks,
    Layers,
    Columns,
    Id,
    Clips,
    Connected,
    Selected,
    Value,
    Other,
}

const KEYS: [(&[u8], Key); 9] = [
    (b"name", Key::Name),
    (b"decks", Key::Decks),
    (b"layers", Key::Layers),
    (b"columns", Key::Columns),
    (b"id", Key::Id),
    (b"clips", Key::Clips),
    (b"connected", Key::Connected),
    (b"selected", Key::Selected),
    (b"value", Key::Value),
];

/// A parameter object `{"value": …, "id": …}`.
#[derive(Default)]
struct Param {
    string: Option<String>,
    boolean: Option<bool>,
    id: Option<i64>,
}

struct Scanner<'a> {
    p: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    // MARK: composition shape

    fn composition(&mut self) -> R<CompositionSnapshot> {
        let mut comp_name = String::new();
        let mut deck_name = String::new();
        let mut layers: Vec<LayerInfo> = Vec::new();
        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut saw_layers = false;
        let mut saw_columns = false;
        self.object(|s, key| {
            match key {
                Key::Name => comp_name = s.param()?.string.unwrap_or_default(),
                Key::Decks => s.array(|s| {
                    let mut name: Option<String> = None;
                    let mut selected = false;
                    s.object(|s, k| {
                        match k {
                            Key::Name => name = s.param()?.string,
                            Key::Selected => selected = s.param()?.boolean.unwrap_or(false),
                            _ => s.skip_value()?,
                        }
                        Ok(())
                    })?;
                    if selected && deck_name.is_empty() {
                        deck_name = name.unwrap_or_default();
                    }
                    Ok(())
                })?,
                Key::Layers => {
                    saw_layers = true;
                    s.array(|s| {
                        let layer = s.layer(layers.len() + 1)?;
                        layers.push(layer);
                        Ok(())
                    })?;
                }
                Key::Columns => {
                    saw_columns = true;
                    s.array(|s| {
                        let mut id: Option<i64> = None;
                        let mut name = Param::default();
                        s.object(|s, k| {
                            match k {
                                Key::Id => id = s.int64()?,
                                Key::Name => name = s.param()?,
                                _ => s.skip_value()?,
                            }
                            Ok(())
                        })?;
                        let Some(id) = id else { return Err(s.fail("column without id")) };
                        columns.push(ColumnInfo {
                            index: columns.len() + 1,
                            id,
                            raw_name: name.string.unwrap_or_else(|| "Column #".to_string()),
                            name_param_id: name.id,
                        });
                        Ok(())
                    })?;
                }
                _ => s.skip_value()?,
            }
            Ok(())
        })?;
        if !(saw_layers && saw_columns) {
            return Err(self.fail("no layers/columns"));
        }
        Ok(CompositionSnapshot { composition_name: comp_name, deck_name, layers, columns })
    }

    fn layer(&mut self, index: usize) -> R<LayerInfo> {
        let mut id: Option<i64> = None;
        let mut name = Param::default();
        let mut slots: Vec<SlotInfo> = Vec::new();
        self.object(|s, k| {
            match k {
                Key::Id => id = s.int64()?,
                Key::Name => name = s.param()?,
                Key::Clips => s.array(|s| {
                    let mut cid: Option<i64> = None;
                    let mut cname: Option<String> = None;
                    let mut state = "Empty".to_string();
                    s.object(|s, k| {
                        match k {
                            Key::Id => cid = s.int64()?,
                            Key::Name => cname = s.param()?.string,
                            Key::Connected => {
                                state = s.param()?.string.unwrap_or_else(|| "Empty".to_string());
                            }
                            _ => s.skip_value()?,
                        }
                        Ok(())
                    })?;
                    let Some(cid) = cid else { return Err(s.fail("clip without id")) };
                    let clip_name = if state == "Empty" { None } else { cname.filter(|n| !n.is_empty()) };
                    slots.push(SlotInfo { column: slots.len() + 1, clip_id: cid, clip_name, state });
                    Ok(())
                })?,
                _ => s.skip_value()?,
            }
            Ok(())
        })?;
        let Some(id) = id else { return Err(self.fail("layer without id")) };
        Ok(LayerInfo { index, id, raw_name: name.string.unwrap_or_else(|| "Layer #".to_string()), name_param_id: name.id, slots })
    }

    /// A parameter object `{"value": ..., "id": ...}`; other members skipped.
    fn param(&mut self) -> R<Param> {
        let mut out = Param::default();
        if self.peek_null() {
            self.skip_value()?;
            return Ok(out);
        }
        self.object(|s, k| {
            match k {
                Key::Value => match s.peek() {
                    b'"' => out.string = Some(s.string()?),
                    b't' | b'f' => out.boolean = Some(s.boolean()?),
                    _ => s.skip_value()?,
                },
                Key::Id => out.id = s.int64()?,
                _ => s.skip_value()?,
            }
            Ok(())
        })?;
        Ok(out)
    }

    // MARK: keys

    /// Reads a member key (no unescaping needed: wanted keys are plain ASCII).
    fn key(&mut self) -> R<Key> {
        self.expect(b'"')?;
        let start = self.i;
        while self.i < self.p.len() && self.p[self.i] != b'"' {
            if self.p[self.i] == b'\\' {
                self.i += 1;
            }
            self.i += 1;
        }
        if self.i >= self.p.len() {
            return Err(self.fail("unterminated key"));
        }
        let raw = &self.p[start..self.i];
        self.i += 1;
        Ok(KEYS.iter().find(|(bytes, _)| *bytes == raw).map_or(Key::Other, |(_, k)| *k))
    }

    // MARK: structure

    fn object(&mut self, mut member: impl FnMut(&mut Self, Key) -> R<()>) -> R<()> {
        self.expect(b'{')?;
        self.ws();
        if self.peek() == b'}' {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.ws();
            let k = self.key()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            member(self, k)?;
            self.ws();
            let Some(&c) = self.p.get(self.i) else {
                return Err(self.fail("unterminated object"));
            };
            self.i += 1;
            if c == b'}' {
                return Ok(());
            }
            if c != b',' {
                return Err(self.fail("expected , or }"));
            }
        }
    }

    fn array(&mut self, mut element: impl FnMut(&mut Self) -> R<()>) -> R<()> {
        if self.peek_null() {
            return self.skip_value();
        }
        self.expect(b'[')?;
        self.ws();
        if self.peek() == b']' {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.ws();
            element(self)?;
            self.ws();
            let Some(&c) = self.p.get(self.i) else {
                return Err(self.fail("unterminated array"));
            };
            self.i += 1;
            if c == b']' {
                return Ok(());
            }
            if c != b',' {
                return Err(self.fail("expected , or ]"));
            }
        }
    }

    /// Skips one value of any kind. Containers are skipped by a depth count that ignores
    /// brackets inside strings.
    fn skip_value(&mut self) -> R<()> {
        self.ws();
        let Some(&c) = self.p.get(self.i) else { return Err(self.fail("value expected")) };
        if c == b'"' {
            return self.skip_string();
        }
        if c == b'{' || c == b'[' {
            let mut depth = 0usize;
            while let Some(&b) = self.p.get(self.i) {
                match b {
                    b'"' => {
                        self.skip_string()?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            self.i += 1;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
                self.i += 1;
            }
            return Err(self.fail("unterminated container"));
        }
        // number / true / false / null
        while let Some(&b) = self.p.get(self.i) {
            if b == b',' || b == b'}' || b == b']' || b <= 0x20 {
                return Ok(());
            }
            self.i += 1;
        }
        Ok(())
    }

    fn skip_string(&mut self) -> R<()> {
        self.i += 1; // opening quote
        while let Some(&b) = self.p.get(self.i) {
            if b == b'\\' {
                self.i += 2;
                continue;
            }
            self.i += 1;
            if b == b'"' {
                return Ok(());
            }
        }
        Err(self.fail("unterminated string"))
    }

    // MARK: scalars

    fn string(&mut self) -> R<String> {
        self.expect(b'"')?;
        let start = self.i;
        let mut escaped = false;
        while self.i < self.p.len() && self.p[self.i] != b'"' {
            if self.p[self.i] == b'\\' {
                escaped = true;
                self.i += 1;
            }
            self.i += 1;
        }
        if self.i >= self.p.len() {
            return Err(self.fail("unterminated string"));
        }
        let raw = &self.p[start..self.i];
        self.i += 1;
        if !escaped {
            return Ok(String::from_utf8_lossy(raw).into_owned());
        }
        self.unescape(raw)
    }

    fn unescape(&self, raw: &[u8]) -> R<String> {
        let hex4 = |at: usize| -> R<u32> {
            let Some(chunk) = raw.get(at..at + 4) else { return Err(self.fail("bad \\u escape")) };
            let mut v: u32 = 0;
            for &h in chunk {
                let d = match h {
                    b'0'..=b'9' => h - b'0',
                    b'A'..=b'F' => h - b'A' + 10,
                    b'a'..=b'f' => h - b'a' + 10,
                    _ => return Err(self.fail("bad \\u escape")),
                };
                v = (v << 4) | u32::from(d);
            }
            Ok(v)
        };
        let mut out: Vec<u8> = Vec::with_capacity(raw.len());
        let mut j = 0;
        while j < raw.len() {
            let b = raw[j];
            if b != b'\\' || j + 1 >= raw.len() {
                out.push(b);
                j += 1;
                continue;
            }
            let e = raw[j + 1];
            j += 2;
            match e {
                b'n' => out.push(0x0A),
                b't' => out.push(0x09),
                b'r' => out.push(0x0D),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0C),
                b'u' => {
                    let mut scalar = hex4(j)?;
                    j += 4;
                    if (0xD800..=0xDBFF).contains(&scalar) && j + 6 <= raw.len() && raw[j] == b'\\' && raw[j + 1] == b'u' {
                        let low = hex4(j + 2)?;
                        if (0xDC00..=0xDFFF).contains(&low) {
                            scalar = 0x10000 + ((scalar - 0xD800) << 10) + (low - 0xDC00);
                            j += 6;
                        }
                    }
                    let c = char::from_u32(scalar).unwrap_or('\u{FFFD}');
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                other => out.push(other), // \" \\ \/
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn int64(&mut self) -> R<Option<i64>> {
        self.ws();
        if self.peek_null() {
            self.skip_value()?;
            return Ok(None);
        }
        let mut neg = false;
        if self.peek() == b'-' {
            neg = true;
            self.i += 1;
        }
        let mut v: i64 = 0;
        let mut digits = 0;
        while let Some(&d) = self.p.get(self.i) {
            if !d.is_ascii_digit() {
                break;
            }
            v = match v.checked_mul(10).and_then(|m| m.checked_add(i64::from(d - b'0'))) {
                Some(n) => n,
                None => return Err(self.fail("integer overflow")),
            };
            digits += 1;
            self.i += 1;
        }
        if digits == 0 {
            return Err(self.fail("number expected"));
        }
        // A fraction/exponent is not an id: consume it and give up on the value.
        if matches!(self.p.get(self.i), Some(b'.' | b'e' | b'E')) {
            self.skip_value()?;
            return Ok(None);
        }
        Ok(Some(if neg { -v } else { v }))
    }

    fn boolean(&mut self) -> R<bool> {
        if self.matches(b"true") {
            self.i += 4;
            return Ok(true);
        }
        if self.matches(b"false") {
            self.i += 5;
            return Ok(false);
        }
        Err(self.fail("bool expected"))
    }

    // MARK: bytes

    fn ws(&mut self) {
        while matches!(self.p.get(self.i), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.i += 1;
        }
    }

    fn peek(&self) -> u8 {
        self.p.get(self.i).copied().unwrap_or(0)
    }

    fn peek_null(&mut self) -> bool {
        self.ws();
        self.matches(b"null")
    }

    fn matches(&self, word: &[u8]) -> bool {
        self.p.get(self.i..self.i + word.len()) == Some(word)
    }

    fn expect(&mut self, b: u8) -> R<()> {
        if self.p.get(self.i) != Some(&b) {
            return Err(self.fail(&format!("expected '{}'", char::from(b))));
        }
        self.i += 1;
        Ok(())
    }

    fn fail(&self, message: &str) -> Failure {
        Failure { message: message.to_string(), offset: self.i }
    }
}
