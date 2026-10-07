//! Glyph names from TeX's maths fonts (Computer Modern's CMMI, CMSY, CMEX, the AMS fonts MSAM and MSBM, and
//! the Latin Modern and TX/PX maths fonts that copy their names) that the Adobe Glyph List doesn't have,
//! and the private-use codes Adobe gave the pieces of large brackets.
//!
//! A TeX maths font names a glyph by what it is, and its larger sizes by a suffix: CMEX draws a left
//! parenthesis as parenleftbig, parenleftBig, parenleftbigg and parenleftBigg, a sum as summationtext and
//! summationdisplay, a wide hat as hatwide, hatwider and hatwidest. Such a name is looked up as written,
//! then with its size suffix taken off. The table below was written for this crate from the Unicode
//! meanings of the glyphs; it covers the names found in the maths fonts of arXiv papers.

use crate::tables;

/// TeX maths glyph names the Adobe Glyph List lacks, sorted by name for binary search.
pub static TEX_NAMES: [(&str, &str); 72] = [
    ("Ifractur", "\u{2111}"), ("Rfractur", "\u{211C}"), ("angbracketleft", "\u{27E8}"), ("angbracketright", "\u{27E9}"),
    ("arrowhookleft", "\u{21A9}"), ("arrowhookright", "\u{21AA}"), ("arrowleftbothalf", "\u{21BD}"), ("arrownortheast", "\u{2197}"),
    ("arrownorthwest", "\u{2196}"), ("arrowrighttophalf", "\u{21C0}"), ("arrowsoutheast", "\u{2198}"), ("arrowsouthwest", "\u{2199}"),
    ("arrowtailright", "\u{21A3}"), ("asteriskmath", "\u{2217}"), ("bardbl", "\u{2016}"), ("ceilingleft", "\u{2308}"),
    ("ceilingright", "\u{2309}"), ("circledot", "\u{2299}"), ("coproduct", "\u{2210}"), ("diamondmath", "\u{22C4}"),
    ("dotlessj", "\u{0237}"), ("epsilon1", "\u{03B5}"), ("equivasymptotic", "\u{224D}"), ("flat", "\u{266D}"),
    ("floorleft", "\u{230A}"), ("floorright", "\u{230B}"), ("follows", "\u{227B}"), ("followsequal", "\u{2AB0}"),
    ("greatermuch", "\u{226B}"), ("greaterorequalslant", "\u{2A7E}"), ("greaterorsimilar", "\u{2273}"), ("hat", "\u{02C6}"),
    ("intersectionsq", "\u{2293}"), ("latticebottom", "\u{22A5}"), ("latticetop", "\u{22A4}"), ("lessmuch", "\u{226A}"),
    ("lessorequalslant", "\u{2A7D}"), ("lessorsimilar", "\u{2272}"), ("lscript", "\u{2113}"), ("mapsto", "\u{21A6}"),
    ("minusplus", "\u{2213}"), ("multicloseleft", "\u{22C9}"), ("multicloseright", "\u{22CA}"), ("nabla", "\u{2207}"),
    ("natural", "\u{266E}"), ("negationslash", "\u{0338}"), ("owner", "\u{220B}"), ("phi1", "\u{03C6}"),
    ("pi1", "\u{03D6}"), ("precedes", "\u{227A}"), ("precedesequal", "\u{2AAF}"), ("prime", "\u{2032}"),
    ("rho1", "\u{03F1}"), ("sharp", "\u{266F}"), ("sigma1", "\u{03C2}"), ("simequal", "\u{2243}"),
    ("similarequal", "\u{2243}"), ("square", "\u{25A1}"), ("squaremultiply", "\u{22A0}"), ("star", "\u{22C6}"),
    ("subsetnoteql", "\u{228A}"), ("supersetnoteql", "\u{228B}"), ("theta1", "\u{03D1}"), ("triangle", "\u{25B3}"),
    ("triangleinv", "\u{25BD}"), ("triangleleft", "\u{25C1}"), ("triangleright", "\u{25B7}"), ("unionsq", "\u{2294}"),
    ("vector", "\u{20D7}"), ("vextenddouble", "\u{2225}"), ("vextendsingle", "\u{2223}"), ("wreathproduct", "\u{2240}"),
];

/// The size and piece suffixes TeX's extension fonts add to a glyph's base name, longest first. A digit
/// may follow (parenleftbig5), and an "A" may end a piece name (parenlefttpA).
const SUFFIXES: [&str; 13] = ["display", "widest", "wider", "vertex", "text", "wide", "Bigg", "bigg", "Big", "big", "mid", "tp", "bt"];

/// The base names that come in sizes or pieces in TeX's extension fonts. Only these lose a suffix: a
/// subset font's arbitrary names (G12, C34, a17) must stay unmapped, not become letters.
const SIZED: [&str; 31] = [
    "angbracketleft", "angbracketright", "backslash", "bar", "braceleft", "braceright", "bracketleft",
    "bracketright", "ceilingleft", "ceilingright", "circledot", "circlemultiply", "circleplus", "coproduct",
    "floorleft", "floorright", "hat", "integral", "intersection", "logicaland", "logicalor", "parenleft",
    "parenright", "product", "radical", "slash", "summation", "tilde", "union", "unionsq", "vextendsingle",
];

fn exact(name: &str) -> Option<&'static str> {
    if let Ok(k) = TEX_NAMES.binary_search_by(|(n, _)| n.cmp(&name)) { return Some(TEX_NAMES[k].1); }
    tables::AGL.binary_search_by(|(n, _)| n.cmp(&name)).ok().map(|k| tables::AGL[k].1)
}

/// Unicode for a TeX maths glyph name the Adobe Glyph List doesn't have: from the table, or, for a glyph
/// that comes in sizes, from its base name (parenleftbig5, summationdisplay, hatwide, parenlefttpA).
pub fn tex_unicode(name: &str) -> Option<String> {
    if let Ok(k) = TEX_NAMES.binary_search_by(|(n, _)| n.cmp(&name)) { return Some(TEX_NAMES[k].1.to_string()); }
    // a bracket piece Adobe named (parenlefttp, bracketrightbt), with TeX's trailing "A"
    if let Some(piece) = name.strip_suffix('A') {
        if let Some(u) = tables::AGL.binary_search_by(|(n, _)| n.cmp(&piece)).ok().map(|k| tables::AGL[k].1) {
            return Some(u.to_string());
        }
    }
    let bare = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = SUFFIXES.iter().find_map(|s| bare.strip_suffix(s))?;
    if bare.len() < name.len() && !SUFFIXES.iter().any(|s| bare.ends_with(s)) { return None; }
    if !SIZED.contains(&base) { return None; }
    // the Adobe Glyph List's own private-use pieces stay as they are: widths are keyed by them, and
    // Font::unicode folds them on the way out
    exact(base).map(|u| u.to_string())
}

/// Adobe's private-use codes for the pieces of large brackets and braces (the Symbol font's U+F8E5 to
/// U+F8FE, which the Adobe Glyph List gives names such as parenlefttp) as the standard Unicode pieces.
pub fn fold_private(s: &str) -> String {
    if !s.chars().any(|c| ('\u{f8e5}'..='\u{f8fe}').contains(&c)) { return s.to_string(); }
    s.chars().map(|c| match c {
        '\u{f8e5}' => '\u{203E}', '\u{f8e6}' => '\u{23D0}', '\u{f8e7}' => '\u{23AF}', '\u{f8e8}' => '\u{00AE}',
        '\u{f8e9}' => '\u{00A9}', '\u{f8ea}' => '\u{2122}', '\u{f8eb}' => '\u{239B}', '\u{f8ec}' => '\u{239C}',
        '\u{f8ed}' => '\u{239D}', '\u{f8ee}' => '\u{23A1}', '\u{f8ef}' => '\u{23A2}', '\u{f8f0}' => '\u{23A3}',
        '\u{f8f1}' => '\u{23A7}', '\u{f8f2}' => '\u{23A8}', '\u{f8f3}' => '\u{23A9}', '\u{f8f4}' => '\u{23AA}',
        '\u{f8f5}' => '\u{23AE}', '\u{f8f6}' => '\u{239E}', '\u{f8f7}' => '\u{239F}', '\u{f8f8}' => '\u{23A0}',
        '\u{f8f9}' => '\u{23A4}', '\u{f8fa}' => '\u{23A5}', '\u{f8fb}' => '\u{23A6}', '\u{f8fc}' => '\u{23AB}',
        '\u{f8fd}' => '\u{23AC}', '\u{f8fe}' => '\u{23AD}', other => other,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_sorted_for_binary_search() {
        assert!(TEX_NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(SIZED.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn tex_names_and_sized_glyphs_decode() {
        assert_eq!(tex_unicode("prime").as_deref(), Some("\u{2032}"));
        assert_eq!(tex_unicode("epsilon1").as_deref(), Some("\u{03B5}"));
        assert_eq!(tex_unicode("summationdisplay").as_deref(), Some("\u{2211}"));
        assert_eq!(tex_unicode("parenleftBigg").as_deref(), Some("("));
        assert_eq!(tex_unicode("parenleftbig5").as_deref(), Some("("));
        assert_eq!(tex_unicode("integraltext").as_deref(), Some("\u{222B}"));
        assert_eq!(tex_unicode("angbracketleftBig").as_deref(), Some("\u{27E8}"));
        assert_eq!(tex_unicode("parenlefttpA").map(|u| fold_private(&u)).as_deref(), Some("\u{239B}"));
        assert_eq!(tex_unicode("nothinglikethis"), None);
        // a subset font's arbitrary names stay unmapped
        for name in ["G12", "C34", "a17", "i5", "H2", "g42", "uni", "u1"] { assert_eq!(tex_unicode(name), None, "{name}"); }
    }

    #[test]
    fn adobe_bracket_pieces_become_unicode_pieces() {
        assert_eq!(fold_private("\u{f8eb}\u{f8ec}\u{f8ed}"), "\u{239B}\u{239C}\u{239D}");
        assert_eq!(fold_private("plain text"), "plain text");
    }
}
