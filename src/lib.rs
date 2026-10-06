//! The PDF reading shared by scan-or-text, wordbox and where-are-the-regions: byte helpers, values
//! inside dictionaries, the stream filters (Flate, LZW, RunLength, ASCII85, ASCIIHex), the object
//! index with object streams and decryption, the page tree, and the fonts (font programs, encodings,
//! CMaps, glyph outlines). Moved from where-are-the-regions
//! without changing what it does; the tools used to carry a copy each.

use std::collections::HashMap;

pub mod crypt;
// fonts: programs, encodings, CMaps and glyph outlines, shared by wordbox and where-are-the-regions
pub mod cff;
pub mod cmap;
pub mod font;
pub mod outline;
pub mod tables;
pub mod truetype;
pub mod type1;

/// How far the page tree is walked.
pub const MAX_PAGES: usize = 2000;

// ---------- low-level byte helpers ----------

pub fn is_ws(b: u8) -> bool { matches!(b, b' ' | b'\n' | b'\r' | b'\t' | 0x0c | 0) }
pub fn is_delim(b: u8) -> bool { matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%') }
pub fn is_regular(b: u8) -> bool { !is_ws(b) && !is_delim(b) }

pub fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() { return None; }
    let first = needle[0];
    let last = hay.len().checked_sub(needle.len())?;
    let mut i = from;
    while i <= last {
        match hay[i..=last].iter().position(|&b| b == first) {
            None => return None,
            Some(off) => {
                i += off;
                if &hay[i..i + needle.len()] == needle { return Some(i); }
                i += 1;
            }
        }
    }
    None
}

pub fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() { return None; }
    (0..=hay.len() - needle.len()).rev().find(|&i| &hay[i..i + needle.len()] == needle)
}

pub fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() {
        if is_ws(s[i]) { i += 1; }
        else if s[i] == b'%' { while i < s.len() && s[i] != b'\n' && s[i] != b'\r' { i += 1; } }
        else { break; }
    }
    i
}

pub fn parse_uint(s: &[u8], i: usize) -> Option<(u64, usize)> {
    let mut j = i;
    let mut v: u64 = 0;
    while j < s.len() && s[j].is_ascii_digit() { v = v.saturating_mul(10).saturating_add((s[j] - b'0') as u64); j += 1; }
    if j == i { None } else { Some((v, j)) }
}

/// Index of the matching close for a balanced "<<...>>" or "[...]" starting at `i` (at the opener).
pub fn matching(s: &[u8], i: usize) -> usize {
    let mut depth = 0i32;
    let mut j = i;
    while j < s.len() {
        match s[j] {
            b'(' => { j = skip_string(s, j); continue; }
            b'<' if j + 1 < s.len() && s[j + 1] == b'<' => { depth += 1; j += 2; continue; }
            b'>' if j + 1 < s.len() && s[j + 1] == b'>' => { depth -= 1; j += 2; if depth == 0 { return j; } continue; }
            b'[' => depth += 1,
            b']' => { depth -= 1; if depth == 0 { return j + 1; } }
            _ => {}
        }
        j += 1;
    }
    s.len()
}

/// Skip a literal string "(...)" starting at `i`; returns the index after it.
pub fn skip_string(s: &[u8], i: usize) -> usize {
    let mut depth = 0i32;
    let mut j = i;
    while j < s.len() {
        match s[j] {
            b'\\' => { j += 2; continue; }
            b'(' => depth += 1,
            b')' => { depth -= 1; if depth == 0 { return j + 1; } }
            _ => {}
        }
        j += 1;
    }
    s.len()
}

// ---------- values inside dictionaries ----------

#[derive(Clone, Debug)]
pub enum Val { Ref(u32), Num(f64), Name(Vec<u8>), Dict(Vec<u8>), Array(Vec<u8>), Other }

pub fn parse_val(s: &[u8], i: usize) -> Val {
    let i = skip_ws(s, i);
    if i >= s.len() { return Val::Other; }
    match s[i] {
        b'/' => {
            let mut j = i + 1;
            while j < s.len() && is_regular(s[j]) { j += 1; }
            Val::Name(s[i + 1..j].to_vec())
        }
        b'<' if i + 1 < s.len() && s[i + 1] == b'<' => { let e = matching(s, i); Val::Dict(s[i..e.min(s.len())].to_vec()) }
        b'[' => { let e = matching(s, i); Val::Array(s[i + 1..e.saturating_sub(1).max(i + 1)].to_vec()) }
        b'0'..=b'9' => {
            if let Some((n, j)) = parse_uint(s, i) {
                let k = skip_ws(s, j);
                if let Some((_, k2)) = parse_uint(s, k) {
                    let k3 = skip_ws(s, k2);
                    if k3 < s.len() && s[k3] == b'R' && (k3 + 1 >= s.len() || !is_regular(s[k3 + 1])) { return Val::Ref(n as u32); }
                }
                return parse_num(s, i).map(Val::Num).unwrap_or(Val::Other);
            }
            Val::Other
        }
        b'-' | b'+' | b'.' => parse_num(s, i).map(Val::Num).unwrap_or(Val::Other),
        _ => Val::Other,
    }
}

pub fn parse_num(s: &[u8], i: usize) -> Option<f64> {
    let mut j = i;
    while j < s.len() && (s[j].is_ascii_digit() || matches!(s[j], b'-' | b'+' | b'.')) { j += 1; }
    std::str::from_utf8(&s[i..j]).ok()?.parse().ok()
}

/// Value of a top-level key in a dictionary's bytes (nested dictionaries are skipped).
pub fn get(dict: &[u8], key: &[u8]) -> Option<Val> {
    let mut i = if dict.starts_with(b"<<") { 2 } else { 0 };
    while i < dict.len() {
        i = skip_ws(dict, i);
        if i >= dict.len() { break; }
        match dict[i] {
            b'/' => {
                let mut j = i + 1;
                while j < dict.len() && is_regular(dict[j]) { j += 1; }
                let k = &dict[i..j];
                let v = parse_val(dict, j);
                if k == key { return Some(v); }
                i = skip_val(dict, j);
            }
            b'>' => break,
            _ => i += 1,
        }
    }
    None
}

pub fn skip_val(s: &[u8], i: usize) -> usize {
    let i = skip_ws(s, i);
    if i >= s.len() { return i; }
    match s[i] {
        b'<' if i + 1 < s.len() && s[i + 1] == b'<' => matching(s, i),
        b'[' => matching(s, i),
        b'(' => skip_string(s, i),
        b'<' => find(s, b">", i).map(|e| e + 1).unwrap_or(s.len()),
        b'/' => { let mut j = i + 1; while j < s.len() && is_regular(s[j]) { j += 1; } j }
        _ => {
            // a number, or "N G R"
            let mut j = i;
            while j < s.len() && is_regular(s[j]) { j += 1; }
            let k = skip_ws(s, j);
            if let Some((_, k2)) = parse_uint(s, k) {
                let k3 = skip_ws(s, k2);
                if k3 < s.len() && s[k3] == b'R' { return k3 + 1; }
            }
            j
        }
    }
}

pub fn refs_in(s: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        i = skip_ws(s, i);
        if i >= s.len() { break; }
        if s[i].is_ascii_digit() {
            if let Val::Ref(n) = parse_val(s, i) { out.push(n); }
        }
        i = skip_val(s, i).max(i + 1);
    }
    out
}

pub fn nums_in(s: &[u8]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        i = skip_ws(s, i);
        if i < s.len() && (s[i].is_ascii_digit() || matches!(s[i], b'-' | b'+' | b'.')) {
            if let Some(v) = parse_num(s, i) { out.push(v); }
        }
        i = skip_val(s, i).max(i + 1);
    }
    out
}

/// ASCII85 (base-85) decoding, up to the "~>" end marker.
pub fn ascii85(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 4 / 5);
    let (mut acc, mut n) = (0u64, 0usize);
    let mut i = if s.starts_with(b"<~") { 2 } else { 0 };
    while i < s.len() {
        let c = s[i];
        i += 1;
        match c {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                acc = acc * 85 + (c - b'!') as u64;
                n += 1;
                if n == 5 { out.extend_from_slice(&(acc as u32).to_be_bytes()); acc = 0; n = 0; }
            }
            _ if is_ws(c) => {}
            _ => return None,
        }
    }
    if n > 1 {
        for _ in n..5 { acc = acc * 85 + 84; }
        out.extend_from_slice(&(acc as u32).to_be_bytes()[..n - 1]);
    }
    Some(out)
}

/// LZW decoding as PDF uses it (ISO 32000-1 7.4.4): 9 to 12 bit codes, 256 = clear, 257 = end.
/// With `early` (the default EarlyChange 1), the code width grows one code sooner.
pub fn lzw(data: &[u8], early: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 3);
    // each entry is (prefix entry, last byte, first byte, length); strings are rebuilt on output
    let mut table: Vec<(u32, u8, u8, u32)> = (0..256u32).map(|b| (u32::MAX, b as u8, b as u8, 1)).collect();
    table.push((0, 0, 0, 0));
    table.push((0, 0, 0, 0));
    let (mut width, mut buf, mut nbits, mut i) = (9u32, 0u32, 0u32, 0usize);
    let mut prev: Option<u32> = None;
    let mut scratch = Vec::new();
    loop {
        while nbits < width {
            if i >= data.len() { return out; }
            buf = (buf << 8) | data[i] as u32;
            i += 1;
            nbits += 8;
        }
        let code = (buf >> (nbits - width)) & ((1 << width) - 1);
        nbits -= width;
        buf &= (1 << nbits) - 1;
        if code == 256 { table.truncate(258); width = 9; prev = None; continue; }
        if code == 257 { break; }
        let known = (code as usize) < table.len();
        let (first, entry) = match (known, prev) {
            (true, _) => (table[code as usize].2, code),
            (false, Some(p)) if code as usize == table.len() => (table[p as usize].2, u32::MAX),
            _ => break,
        };
        if let Some(p) = prev {
            let pe = table[p as usize];
            if table.len() < 4096 { table.push((p, first, pe.2, pe.3 + 1)); }
        }
        let e = if entry == u32::MAX { (table.len() - 1) as u32 } else { entry };
        // walk the entry back to its root, then reverse
        scratch.clear();
        let mut k = e;
        while k != u32::MAX { let t = table[k as usize]; scratch.push(t.1); k = t.0; }
        out.extend(scratch.iter().rev());
        prev = Some(e);
        let size = table.len() as u32 + if early { 1 } else { 0 };
        if size >= (1 << width) && width < 12 { width += 1; }
    }
    out
}

/// RunLengthDecode (ISO 32000-1 7.4.5).
pub fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    let mut i = 0;
    while i < data.len() {
        let n = data[i] as usize;
        i += 1;
        if n < 128 { let e = (i + n + 1).min(data.len()); out.extend_from_slice(&data[i..e]); i = e; }
        else if n > 128 { if i < data.len() { out.extend(std::iter::repeat(data[i]).take(257 - n)); } i += 1; }
        else { break; }
    }
    out
}

/// A string value (literal "(...)" with escapes, or hex "<...>") at position `i`.
pub fn string_at(s: &[u8], i: usize) -> Option<Vec<u8>> {
    if i >= s.len() { return None; }
    if s[i] == b'<' {
        let e = find(s, b">", i)?;
        let hex: Vec<u8> = s[i + 1..e].iter().copied().filter(|b| b.is_ascii_hexdigit()).collect();
        let val = |c: u8| (c as char).to_digit(16).unwrap() as u8;
        return Some(hex.chunks(2).map(|p| (val(p[0]) << 4) | if p.len() > 1 { val(p[1]) } else { 0 }).collect());
    }
    if s[i] != b'(' { return None; }
    let (mut out, mut depth, mut j) = (Vec::new(), 1i32, i + 1);
    while j < s.len() {
        let c = s[j];
        match c {
            b'\\' if j + 1 < s.len() => {
                j += 1;
                match s[j] {
                    b'n' => out.push(b'\n'), b'r' => out.push(b'\r'), b't' => out.push(b'\t'),
                    b'b' => out.push(8), b'f' => out.push(12),
                    b'\r' => { if j + 1 < s.len() && s[j + 1] == b'\n' { j += 1; } }
                    b'\n' => {}
                    d @ b'0'..=b'7' => {
                        let mut v = (d - b'0') as u32;
                        for _ in 0..2 { if j + 1 < s.len() && (b'0'..=b'7').contains(&s[j + 1]) { j += 1; v = v * 8 + (s[j] - b'0') as u32; } }
                        out.push(v as u8);
                    }
                    other => out.push(other),
                }
            }
            b'(' => { depth += 1; out.push(c); }
            b')' => { depth -= 1; if depth == 0 { return Some(out); } out.push(c); }
            _ => out.push(c),
        }
        j += 1;
    }
    Some(out)
}

/// A top-level string value of `key` in a dictionary.
pub fn get_string(dict: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let mut i = if dict.starts_with(b"<<") { 2 } else { 0 };
    while i < dict.len() {
        i = skip_ws(dict, i);
        if i >= dict.len() || dict[i] != b'/' { i += 1; continue; }
        let mut j = i + 1;
        while j < dict.len() && is_regular(dict[j]) { j += 1; }
        if &dict[i..j] == key { return string_at(dict, skip_ws(dict, j)); }
        i = skip_val(dict, j);
    }
    None
}

// ---------- the object index ----------

enum Loc {
    Top { gen: u16, dict: (usize, usize), stream: Option<(usize, usize)> },
    Packed { buf: usize, start: usize, end: usize },
}

pub struct Pdf<'a> {
    data: &'a [u8],
    bufs: Vec<Vec<u8>>,
    objs: HashMap<u32, Loc>,
    pub crypt: Option<crypt::Crypt>,
}

impl<'a> Pdf<'a> {
    pub fn index(data: &'a [u8]) -> Pdf<'a> {
        let mut objs = HashMap::new();
        let mut i = 0;
        while let Some(p) = find(data, b"obj", i) {
            i = p + 3;
            if p == 0 || !is_ws(data[p - 1]) { continue; }
            if p + 3 < data.len() && is_regular(data[p + 3]) { continue; }
            // walk back over "N G "
            let mut j = p - 1;
            while j > 0 && is_ws(data[j]) { j -= 1; }
            let gen_end = j + 1;
            while j > 0 && data[j].is_ascii_digit() { j -= 1; }
            if j + 1 == gen_end || !is_ws(data[j]) { continue; }
            let gen = parse_uint(data, j + 1).map(|(g, _)| g as u16).unwrap_or(0);
            while j > 0 && is_ws(data[j]) { j -= 1; }
            let num_end = j + 1;
            while j > 0 && data[j].is_ascii_digit() { j -= 1; }
            let num_start = if data[j].is_ascii_digit() { j } else { j + 1 };
            if num_start == num_end { continue; }
            let num = match parse_uint(data, num_start) { Some((n, _)) => n as u32, None => continue };
            let start = p + 3;
            let end = find(data, b"endobj", start).unwrap_or(data.len());
            // search only inside this object: an unbounded search runs to the next stream in the file,
            // which is quadratic in files with many small objects
            let stream = find(&data[..end], b"stream", start);
            let loc = match stream {
                Some(k) => {
                    let mut s = k + 6;
                    if s < data.len() && data[s] == b'\r' { s += 1; }
                    if s < data.len() && data[s] == b'\n' { s += 1; }
                    let e = rfind(&data[s..end], b"endstream").map(|x| s + x).unwrap_or(end);
                    Loc::Top { gen, dict: (start, k), stream: Some((s, e)) }
                }
                None => Loc::Top { gen, dict: (start, end), stream: None },
            };
            objs.insert(num, loc);
            i = end.max(i);
        }
        let mut pdf = Pdf { data, bufs: Vec::new(), objs, crypt: None };
        pdf.crypt = pdf.security_handler();
        pdf.unpack_object_streams();
        pdf
    }

    /// The standard security handler, when the file opens with an empty user password.
    pub fn security_handler(&self) -> Option<crypt::Crypt> {
        let p = rfind(self.data, b"/Encrypt")?;
        let enc = match parse_val(self.data, p + 8) { Val::Ref(n) => self.dict(n)?, Val::Dict(d) => d, _ => return None };
        if !matches!(get(&enc, b"/Filter"), Some(Val::Name(f)) if f == b"Standard") { return None; }
        let r = match get(&enc, b"/R") { Some(Val::Num(v)) => v as u32, _ => return None };
        // Key length in bits: the top-level /Length if present; revision 4 files usually give it only
        // inside the crypt filter (/CF /StdCF /Length, often in bytes) and AESV2 is always 128.
        let length = match get(&enc, b"/Length") {
            Some(Val::Num(v)) => v as u32,
            _ if r >= 4 => {
                let inner = find(&enc, b"/StdCF", 0).and_then(|k| find(&enc, b"/Length", k)).map(|k| parse_val(&enc, k + 7));
                match inner { Some(Val::Num(v)) if v <= 32.0 => v as u32 * 8, Some(Val::Num(v)) => v as u32, _ => 128 }
            }
            _ => 40,
        };
        let perms = match get(&enc, b"/P") { Some(Val::Num(v)) => v as i64 as i32, _ => return None };
        let o = get_string(&enc, b"/O")?;
        let aes = find(&enc, b"/AESV2", 0).is_some();
        let encrypt_metadata = !matches!(find(&enc, b"/EncryptMetadata", 0), Some(k) if enc[k..].starts_with(b"/EncryptMetadata false"));
        let idp = rfind(self.data, b"/ID")?;
        let id0 = match parse_val(self.data, idp + 3) { Val::Array(a) => string_at(&a, skip_ws(&a, 0))?, _ => return None };
        crypt::Crypt::new(r, if r == 2 { 40 } else { length }, &o, perms, &id0, aes, encrypt_metadata)
    }

    /// Objects packed in object streams. Deterministic: streams are read in file order, and a packed
    /// copy replaces an earlier one, or a top-level object that sits earlier in the file, as an
    /// incremental update would. (Iterating the HashMap directly made the winner depend on its seed.)
    pub fn unpack_object_streams(&mut self) {
        let mut stms: Vec<(usize, u32)> = self.objs.iter()
            .filter_map(|(n, l)| match l { Loc::Top { dict, stream: Some(_), .. } => Some((dict.0, *n)), _ => None })
            .filter(|(_, n)| matches!(self.dict(*n).and_then(|d| get(&d, b"/Type")), Some(Val::Name(t)) if t == b"ObjStm"))
            .collect();
        stms.sort_unstable();
        // file position of each packed object's stream, for the "newer wins" rule
        let mut packed_at: HashMap<u32, usize> = HashMap::new();
        for (pos, n) in stms {
            let dict = match self.dict(n) { Some(d) => d, None => continue };
            let count = match get(&dict, b"/N") { Some(Val::Num(v)) => v as usize, _ => continue };
            let first = match get(&dict, b"/First") { Some(Val::Num(v)) => v as usize, _ => continue };
            let buf = match self.stream(n) { Some(b) => b, None => continue };
            if first > buf.len() { continue; }
            let header = nums_in(&buf[..first]);
            let idx = self.bufs.len();
            let mut entries = Vec::new();
            for k in 0..count.min(header.len() / 2) {
                let num = header[2 * k] as u32;
                let off = first + header[2 * k + 1] as usize;
                let next = if k + 1 < header.len() / 2 { first + header[2 * k + 3] as usize } else { buf.len() };
                if off <= next && next <= buf.len() { entries.push((num, off, next)); }
            }
            self.bufs.push(buf);
            for (num, start, end) in entries {
                let newer = match self.objs.get(&num) {
                    None => true,
                    Some(Loc::Packed { .. }) => packed_at.get(&num).map(|&p| p < pos).unwrap_or(true),
                    Some(Loc::Top { dict, .. }) => dict.0 < pos,
                };
                if newer {
                    self.objs.insert(num, Loc::Packed { buf: idx, start, end });
                    packed_at.insert(num, pos);
                }
            }
        }
    }

    pub fn dict(&self, n: u32) -> Option<Vec<u8>> {
        let raw: &[u8] = match self.objs.get(&n)? {
            Loc::Top { dict, .. } => &self.data[dict.0..dict.1],
            Loc::Packed { buf, start, end } => &self.bufs[*buf][*start..*end],
        };
        let s = skip_ws(raw, 0);
        if raw[s..].starts_with(b"<<") { let e = matching(raw, s); Some(raw[s..e].to_vec()) } else { Some(raw[s..].to_vec()) }
    }

    pub fn resolve(&self, v: &Val) -> Option<Vec<u8>> {
        match v { Val::Dict(d) => Some(d.clone()), Val::Ref(n) => self.dict(*n), _ => None }
    }

    /// The stream decoded by its whole /Filter chain. None for a filter not handled here.
    pub fn stream(&self, n: u32) -> Option<Vec<u8>> { self.decode(n, false).map(|(v, _)| v) }

    /// The stream decoded by every filter; with `image`, a last filter that is an image codec (DCT,
    /// CCITT, JPX, JBIG2) is left for the caller and named (pixels.rs).
    pub fn decode(&self, n: u32, image: bool) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let (gen, s, e) = match self.objs.get(&n)? { Loc::Top { gen, stream: Some(r), .. } => (*gen, r.0, r.1), _ => return None };
        let dict = self.dict(n)?;
        let is_xref = matches!(get(&dict, b"/Type"), Some(Val::Name(t)) if t == b"XRef");
        let decrypted;
        let raw: &[u8] = match &self.crypt {
            Some(c) if !is_xref => { decrypted = c.decrypt(n, gen, &self.data[s..e.max(s)])?; &decrypted }
            _ => &self.data[s..e.max(s)],
        };
        self.unfilter(&dict, raw.to_vec(), image)
    }

    /// Data run through the /Filter chain of `dict` (an object's dictionary, or an inline image's with its
    /// keys spelt out); with `image`, a last image codec is left for the caller and named, as in decode.
    pub fn unfilter(&self, dict: &[u8], raw: Vec<u8>, image: bool) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let filters: Vec<Vec<u8>> = match get(dict, b"/Filter") {
            None => Vec::new(),
            Some(Val::Name(f)) => vec![f],
            Some(Val::Array(a)) => {
                let mut v = Vec::new();
                let mut i = 0;
                while i < a.len() { if let Val::Name(f) = parse_val(&a, i) { v.push(f); } i = skip_val(&a, i).max(i + 1); }
                v
            }
            Some(Val::Ref(r)) => match self.dict(r) { Some(d) if d.starts_with(b"/") => vec![d[1..].to_vec()], _ => return None },
            _ => return None,
        };
        let mut out = raw;
        let last = filters.len();
        for (k, f) in filters.into_iter().enumerate() {
            if image && k + 1 == last && matches!(f.as_slice(), b"DCTDecode" | b"DCT" | b"CCITTFaxDecode" | b"CCF" | b"JPXDecode" | b"JBIG2Decode") {
                return Some((out, Some(f)));
            }
            if f == b"ASCII85Decode" || f == b"A85" {
                out = ascii85(&out)?;
            } else if f == b"ASCIIHexDecode" || f == b"AHx" {
                let mut v = Vec::with_capacity(out.len() / 2);
                let digits: Vec<u8> = out.iter().copied().take_while(|&b| b != b'>').filter(|b| b.is_ascii_hexdigit()).collect();
                for p in digits.chunks(2) {
                    let h = |c: u8| (c as char).to_digit(16).unwrap() as u8;
                    v.push((h(p[0]) << 4) | if p.len() > 1 { h(p[1]) } else { 0 });
                }
                out = v;
            } else if f == b"LZWDecode" || f == b"LZW" {
                let early = !find(dict, b"/EarlyChange 0", 0).is_some();
                out = lzw(&out, early);
            } else if f == b"RunLengthDecode" || f == b"RL" {
                out = run_length(&out);
            } else if f == b"FlateDecode" || f == b"Fl" {
                out = match miniz_oxide::inflate::decompress_to_vec_zlib(&out) {
                    Ok(v) => v,
                    Err(e) if !e.output.is_empty() => e.output,
                    Err(_) => miniz_oxide::inflate::decompress_to_vec(&out).ok()?,
                };
            } else {
                return None;
            }
        }
        Some((out, None))
    }

    pub fn pages(&self) -> Vec<u32> {
        let mut out = Vec::new();
        if let Some(root) = self.root() {
            if let Some(cat) = self.dict(root) {
                if let Some(Val::Ref(p)) = get(&cat, b"/Pages") {
                    let mut seen = std::collections::HashSet::new();
                    self.walk(p, &mut out, &mut seen, 0);
                }
            }
        }
        if out.is_empty() {
            let mut v: Vec<u32> = self.objs.keys().copied()
                .filter(|n| matches!(self.dict(*n).and_then(|d| get(&d, b"/Type")), Some(Val::Name(t)) if t == b"Page"))
                .collect();
            v.sort_unstable();
            out = v;
        }
        out.truncate(MAX_PAGES);
        out
    }

    pub fn walk(&self, n: u32, out: &mut Vec<u32>, seen: &mut std::collections::HashSet<u32>, depth: usize) {
        if depth > 32 || !seen.insert(n) || out.len() >= MAX_PAGES { return; }
        let d = match self.dict(n) { Some(d) => d, None => return };
        match get(&d, b"/Type") {
            Some(Val::Name(t)) if t == b"Pages" => {
                if let Some(Val::Array(kids)) = get(&d, b"/Kids") { for k in refs_in(&kids) { self.walk(k, out, seen, depth + 1); } }
            }
            _ => {
                if let Some(Val::Array(kids)) = get(&d, b"/Kids") { for k in refs_in(&kids) { self.walk(k, out, seen, depth + 1); } }
                else { out.push(n); }
            }
        }
    }

    pub fn root(&self) -> Option<u32> {
        // the last /Root wins (trailers of incremental updates, or an xref stream's dictionary)
        let p = rfind(self.data, b"/Root")?;
        match parse_val(self.data, p + 5) { Val::Ref(n) => Some(n), _ => None }
    }

    /// A page attribute, inherited through /Parent when missing.
    pub fn inherited(&self, page: u32, key: &[u8]) -> Option<Val> {
        let mut n = page;
        for _ in 0..16 {
            let d = self.dict(n)?;
            if let Some(v) = get(&d, key) { return Some(v); }
            match get(&d, b"/Parent") { Some(Val::Ref(p)) => n = p, _ => return None }
        }
        None
    }
}

impl<'a> Pdf<'a> {
    /// A value with an indirect reference replaced by the object it points to.
    pub fn direct(&self, v: Val) -> Val {
        match v {
            Val::Ref(n) => match self.dict(n) { Some(raw) => parse_val(&raw, 0), None => Val::Other },
            other => other,
        }
    }
}
