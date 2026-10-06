# pdf-core

The part of reading a PDF that every tool in this series needs: finding the objects, decoding their streams and walking the page tree. It's a small Rust library, with no rendering and three dependencies (miniz_oxide for Flate, md-5 and aes for decryption).

scan-or-text, wordbox and where-are-the-regions each used to carry their own copy of this code, and the copies had started to drift apart. It now lives here once and all three depend on it.

## Contents

The object index finds every `N G obj` in the file by scanning for it, so a broken or missing cross-reference table doesn't matter. Objects packed in object streams are unpacked, and when an object is defined more than once the later definition wins, as it does in an incremental update.

Streams go through their /Filter chain: FlateDecode (a truncated stream gives what it decoded, and deflate data without the zlib header is tried too), LZWDecode (with /EarlyChange), RunLengthDecode, ASCII85Decode and ASCIIHexDecode. `decode` can stop before a last image codec (DCT, CCITT, JPX, JBIG2) and say which one, so where-are-the-regions can decode the pixels itself. Any other filter gives `None`.

Files that open with an empty user password are decrypted: the standard security handler, revisions 2 to 4, with RC4 (40 to 128 bit) or AES-128.

`pages` walks the page tree in order, up to 2,000 pages, and `inherited` follows /Parent for attributes like /MediaBox and /Rotate. The rest is byte-level help for reading dictionaries and arrays: `get`, `parse_val`, `skip_val`, `string_at` and so on.

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
| wordbox | the same 1,113 | words and glyphs identical |
| scan-or-text | 1,791 (its 525 routing files, its 153 constructed and sample files, and the 1,113) | page labels identical |

The copies weren't quite the same. wordbox and scan-or-text still decoded streams in one function, which where-are-the-regions had split in two to leave image codecs for the caller; with the image option off it's the same code. scan-or-text unpacked object streams in hash-map order, which changes from run to run, where pdf-core goes in file order; the "later wins" rule gave the same objects either way on every file above. scan-or-text also stopped the page walk at 500 pages. pdf-core goes to 2,000, and scan-or-text now cuts its own list at 500.

## Limits

No AES-256 (revisions 5 and 6) and no user passwords: such a file gets no security handler and its streams don't decode. No JPX or JBIG2 decoding and no colour handling; that's where-are-the-regions' pixels code. The object index finds objects by scanning for `obj`, so binary stream data that happens to read "12 0 obj" between whitespace would be taken for an object.

## Commands

```
cargo build --release
cargo test --release
```

## Licence

MIT. See [LICENSE](LICENSE).
