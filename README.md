# pdf-core

The part of reading a PDF that every tool in this series needs: finding the objects, decoding their streams, walking the page tree and making sense of the fonts. It's a Rust library with no rendering and four dependencies: miniz_oxide for Flate, md-5 and aes for decryption, and ttf-parser for TrueType and CFF outlines.

scan-or-text, wordbox and where-are-the-regions each used to carry their own copy of this code, and the copies had started to drift apart. It now lives here once and all three depend on it.

## Contents

The object index finds every `N G obj` in the file by scanning for it, so a broken or missing cross-reference table doesn't matter. Objects packed in object streams are unpacked, and when an object is defined more than once the later definition wins, as it does in an incremental update.

Streams go through their /Filter chain: FlateDecode (a truncated stream gives what it decoded, and deflate data without the zlib header is tried too), LZWDecode (with /EarlyChange), RunLengthDecode, ASCII85Decode and ASCIIHexDecode. `decode` can stop before a last image codec (DCT, CCITT, JPX, JBIG2) and say which one, so where-are-the-regions can decode the pixels itself. Any other filter gives `None`.

Files that open with an empty user password are decrypted: the standard security handler, revisions 2 to 4, with RC4 (40 to 128 bit) or AES-128.

`pages` walks the page tree in order, up to 2,000 pages, and `inherited` follows /Parent for attributes like /MediaBox and /Rotate. The rest is byte-level help for reading dictionaries and arrays: `get`, `parse_val`, `skip_val`, `string_at` and so on.

`font::Font` turns a character code into text, a width and a box. The text comes from, in this order, the font's ToUnicode map; its encoding (WinAnsi, MacRoman or Standard with /Differences, a Type 1 program's built-in encoding, or Symbol's and ZapfDingbats' own), with glyph names turned into Unicode by the Adobe Glyph List rules; then an embedded TrueType program's own tables. Widths come from /Widths, a CID font's /W, or Adobe's tables for the 14 standard fonts. A font's height comes from its descriptor, with /Descent taken as below the baseline whatever its sign, and for a standard font from Adobe's ascender and descender. For ink boxes it reads glyph outlines: TrueType and CFF through ttf-parser, a CFF glyph ttf-parser rejects through its own Type 2 reader, Type 1 programs through its own reader, a Type 3 glyph from its d1 operands, and a standard font that isn't embedded from Adobe's glyph boxes.

`tex.rs` gives Unicode for glyph names from TeX's maths fonts that the glyph list lacks (prime, epsilon1, summationdisplay, parenleftbig and the like), and turns Adobe's private-use codes for the pieces of large brackets into the standard ones (below).

`tables.rs` holds the encodings, the glyph list, the standard fonts' widths and boxes, and CCITT fax codes for where-are-the-regions' image decoder. `tools/gen_tables.py` writes it from pdfminer.six's copies of the spec tables and Adobe's files in `tools/data/`. Comments that cite a D or GB number refer to where-are-the-regions' results/exact-heldout.md, where the font code was written.

## Use

Each tool depends on it by path, so clone it next to them:

```
pdf-core/
scan-or-text/            classifier/Cargo.toml:   pdf-core = { path = "../../pdf-core" }
wordbox/                 extractor/Cargo.toml:    pdf-core = { path = "../../pdf-core" }
where-are-the-regions/   regions/Cargo.toml:      pdf-core = { path = "../../pdf-core" }
```

The frozen records of where-are-the-regions, wordbox and what-needs-ocr (`results/frozen.sha256`) hold the pdf-core commit they were tested with, and their checks fail if pdf-core has moved on or has uncommitted changes. what-needs-ocr reaches pdf-core through where-are-the-regions.

## The move

The code is where-are-the-regions' copy, the most recent of the three, moved without changing what it does. I checked each tool before and after the switch, running the old and new binaries on the same files:

| Tool | Files | Output |
|---|---|---|
| where-are-the-regions | 1,113 (its constructed tune and held-out sets, govdocs1 003 and 004) | page map and image kinds identical |
| wordbox | 2,254: its 659 dev, 242 held-out and 240 garble files, and the same 1,113 | words and glyphs identical |
| scan-or-text | 1,791 (its 525 routing files, its 153 constructed and sample files, and the 1,113) | page labels identical |

The font code moved second, on 6 October 2026, again where-are-the-regions' copy: its page map and image kinds stayed identical on the 1,113 files. wordbox's copy lacked one fix, Adobe's ascender and descender for the standard fonts, so its word boxes for those fonts changed height. Its scores didn't move, apart from one more reference word found on its dev set; wordbox's results/after-heldout.md has the numbers.

The copies weren't quite the same. wordbox and scan-or-text still decoded streams in one function, which where-are-the-regions had split in two to leave image codecs for the caller; with the image option off it's the same code. scan-or-text unpacked object streams in hash-map order, which changes from run to run, where pdf-core goes in file order; the "later wins" rule gave the same objects either way on every file above. scan-or-text also stopped the page walk at 500 pages. pdf-core goes to 2,000, and scan-or-text now cuts its own list at 500.

## Maths glyph names (7 October 2026)

TeX's maths fonts (Computer Modern's CMMI, CMSY and CMEX, the AMS fonts, Latin Modern Math, the TX and PX fonts) name their glyphs by what they are, and many of those names aren't in the Adobe Glyph List: prime, epsilon1, lscript, mapsto, and every size of the big delimiters and operators, such as parenleftbig, parenleftBigg, summationdisplay and integraltext. Their glyphs came out unmapped. On the first pages of 115 arXiv papers in olmOCR-Bench, 1,219 maths-font words had at least one unmapped glyph, in 82 of the papers.

A name the list lacks is now looked up in a table of 72 such names, written from the glyphs' Unicode meanings. A big-delimiter or big-operator name loses its size suffix (big, Big, bigg, Bigg, text, display, wide, wider, widest, a piece suffix, and a digit after any of these) and is looked up again, but only when what's left is one of 31 base names that come in sizes, such as parenleft, summation and integral. A subset font's arbitrary names, like G12 or a17, stay unmapped. Adobe gave the pieces of large brackets private-use codes (U+F8E5 to U+F8FE); `Font::unicode` now gives the standard pieces (U+239B to U+23AD) instead, while widths are still looked up by the original code, since the Symbol font's widths are keyed by it. A name starting with "u" that isn't a uXXXX code used to fall through to nothing, so unionsq never decoded; it now reaches the table.

On those 115 papers, unmapped maths-font words went from 1,219 to 16. To check that nothing else moved, I ran wordbox before and after on the 1,113 files above: 1,610 words in 19 files gained text, every one of them unmapped before and mapped now; no other word, box or page verdict changed. The characters gained are bracket pieces, parentheses, primes, sums, integrals and the like. Two earlier versions failed that check: one stripped trailing digits from any name, which turned G12 into the letter G and made 30 garbled pages read as text; the other moved Symbol glyphs' boxes, because it changed the code their widths are found by.

## Limits

No AES-256 (revisions 5 and 6) and no user passwords: such a file gets no security handler and its streams don't decode. No JPX or JBIG2 decoding and no colour handling; that's where-are-the-regions' pixels code. The object index finds objects by scanning for `obj`, so binary stream data that happens to read "12 0 obj" between whitespace would be taken for an object. On the font side, vertical writing (Identity-V) isn't handled, and older Type 1 programs whose glyphs are compressed are only partly decoded.

## Commands

```
cargo build --release
cargo test --release
python tools/gen_tables.py > src/tables.rs     # needs pdfminer.six
```

## Licence

MIT. See [LICENSE](LICENSE). ttf-parser is MIT or Apache-2.0. The AFM files in `tools/data/afm/` and `tools/data/zapfdingbats.txt` are Adobe's, under the notices kept with them, and the tables generated from them carry the same notice.
