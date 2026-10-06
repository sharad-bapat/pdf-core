# pdf-core

The PDF reading that scan-or-text, wordbox and where-are-the-regions build on: byte helpers, values inside dictionaries, the stream filters (Flate, LZW, RunLength, ASCII85, ASCIIHex), the object index with object streams, decryption of files that open with an empty password (RC4, AES-128), and the page tree. Rust, three small dependencies (miniz_oxide, md-5, aes).

Each of the tools used to carry its own copy of this code. It now lives here once, and each tool depends on it by path, so clone the repos side by side:

```
pdf-core/
where-are-the-regions/   regions/Cargo.toml: pdf-core = { path = "../../pdf-core" }
```

The move changed nothing the tools do: where-are-the-regions gives the same page map on all 1,113 of its test files and the same image kinds, byte for byte. The tools pin the pdf-core commit they were tested with in their frozen records.

Next: wordbox and scan-or-text move onto it, then the font code (font programs, encodings, CMaps), which wordbox and where-are-the-regions also carry twice.
