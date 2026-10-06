//! Checks of the shared reader on small files written here, so every byte is known.

use pdf_core::*;

/// A PDF from numbered object bodies (the text between "N 0 obj" and "endobj"), with no xref table:
/// the index finds objects by scanning, so none is needed.
fn pdf(objs: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"%PDF-1.5\n".to_vec();
    for (n, body) in objs {
        out.extend_from_slice(format!("{} 0 obj\n", n).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    out.extend_from_slice(b"trailer << /Root 1 0 R >>\n%%EOF\n");
    out
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut v = format!("<< {} /Length {} >>\nstream\n", dict, data.len()).into_bytes();
    v.extend_from_slice(data);
    v.extend_from_slice(b"\nendstream");
    v
}

#[test]
fn run_length_copies_and_repeats() {
    // 2: copy the next 3 bytes; 254: repeat the next byte 257 - 254 = 3 times; 128: end
    assert_eq!(run_length(&[2, b'a', b'b', b'c', 254, b'x', 128, b'z']), b"abcxxx");
}

#[test]
fn lzw_decodes_the_spec_example() {
    // the example in the PDF reference (LZWDecode, early change): "-----A---B"
    assert_eq!(lzw(&[0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01], true), b"-----A---B");
}

#[test]
fn ascii85_decodes() {
    // "Man " is "9jqo^"; "z" is four zero bytes
    assert_eq!(ascii85(b"9jqo^~>").unwrap(), b"Man ");
    assert_eq!(ascii85(b"z~>").unwrap(), [0, 0, 0, 0]);
}

#[test]
fn the_index_reads_pages_streams_and_packed_objects() {
    let text = b"BT /F1 12 Tf 72 700 Td (hello) Tj ET";
    let flate = miniz_oxide::deflate::compress_to_vec_zlib(text, 6);
    // object 7 packed in an object stream: header "7 0", then its dictionary from /First
    let packed = b"7 0 << /Kind /Packed >>";
    let objs = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] /Rotate 90 >>".to_vec()),
        (3, b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".to_vec()),
        (4, b"<< /Type /Page /Parent 2 0 R /Rotate 0 >>".to_vec()),
        (5, stream("/Filter /FlateDecode", &flate)),
        (6, stream("/Type /ObjStm /N 1 /First 4", packed)),
        (8, b"<< /Version 1 >>".to_vec()),
        // object 8 again, later in the file, as an incremental update would write it
        (8, b"<< /Version 2 >>".to_vec()),
    ];
    let data = pdf(&objs);
    let p = Pdf::index(&data);
    assert_eq!(p.pages(), vec![3, 4]);
    // attributes come from the page, or through /Parent when it has none
    assert!(matches!(p.inherited(3, b"/MediaBox"), Some(Val::Array(_))));
    assert!(matches!(p.inherited(3, b"/Rotate"), Some(Val::Num(r)) if r == 90.0));
    assert!(matches!(p.inherited(4, b"/Rotate"), Some(Val::Num(r)) if r == 0.0));
    assert_eq!(p.stream(5).unwrap(), text);
    assert!(matches!(get(&p.dict(7).unwrap(), b"/Kind"), Some(Val::Name(k)) if k == b"Packed"));
    assert!(matches!(get(&p.dict(8).unwrap(), b"/Version"), Some(Val::Num(v)) if v == 2.0));
    assert!(p.crypt.is_none());
}

#[test]
fn decode_can_leave_an_image_codec_to_the_caller() {
    let jpeg_ish = b"\xFF\xD8 not really a jpeg";
    let flate = miniz_oxide::deflate::compress_to_vec_zlib(jpeg_ish, 6);
    let data = pdf(&[(1, stream("/Filter [/FlateDecode /DCTDecode]", &flate))]);
    let p = Pdf::index(&data);
    let (bytes, codec) = p.decode(1, true).unwrap();
    assert_eq!((bytes.as_slice(), codec.as_deref()), (&jpeg_ish[..], Some(&b"DCTDecode"[..])));
    // without the image option a codec it can't decode gives nothing
    assert!(p.stream(1).is_none());
}

#[test]
fn hex_and_chained_filters_unfilter() {
    let p = Pdf::index(b"%PDF-1.4\n%%EOF\n");
    let (out, _) = p.unfilter(b"<< /Filter /ASCIIHexDecode >>", b"48 65 6C 6C 6F>".to_vec(), false).unwrap();
    assert_eq!(out, b"Hello");
    // an odd last digit counts as followed by 0
    let (out, _) = p.unfilter(b"<< /Filter /AHx >>", b"414>".to_vec(), false).unwrap();
    assert_eq!(out, b"A@");
    assert!(p.unfilter(b"<< /Filter /Crypt >>", b"x".to_vec(), false).is_none());
}

#[test]
fn values_parse_from_dictionaries() {
    let d = b"<< /Name /Foo /Num -12.5 /Ref 3 0 R /Arr [1 2 3] /Str (a \\(b\\)) /Sub << /K 1 >> >>";
    assert!(matches!(get(d, b"/Name"), Some(Val::Name(n)) if n == b"Foo"));
    assert!(matches!(get(d, b"/Num"), Some(Val::Num(v)) if v == -12.5));
    assert!(matches!(get(d, b"/Ref"), Some(Val::Ref(3))));
    assert!(matches!(get(d, b"/Arr"), Some(Val::Array(_))));
    assert!(matches!(get(d, b"/Sub"), Some(Val::Dict(_))));
    assert_eq!(get_string(d, b"/Str").unwrap(), b"a (b)");
    // a key that is only the start of another key isn't matched
    assert!(get(b"<< /NameLong 1 >>", b"/Name").is_none());
}
