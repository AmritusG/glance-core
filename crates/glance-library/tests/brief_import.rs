//! `brief_import`: Swift MaCrate 0.16.4's BriefImportTests (same inputs, same expectations), then edge cases whose
//! expected values are what Swift's own `BriefImporter` returned for the same bytes (an oracle built from a scratch
//! copy of BriefImport.swift).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::brief_import::{BriefImportError, BriefImportRecord, MALFORMED_JSON, parse, parse_csv, parse_csv_rows, parse_json};
use std::path::PathBuf;

fn r(clip_id: Option<&str>, path: Option<&str>, brief: &str) -> BriefImportRecord {
    BriefImportRecord { clip_id: clip_id.map(str::to_string), path: path.map(str::to_string), brief: brief.to_string() }
}

fn ids(records: &[BriefImportRecord]) -> Vec<Option<&str>> {
    records.iter().map(|r| r.clip_id.as_deref()).collect()
}

fn malformed() -> Result<Vec<BriefImportRecord>, BriefImportError> {
    Err(BriefImportError::MalformedJson(MALFORMED_JSON.into()))
}

/// A fresh scratch folder under the target dir's temp space.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glance-library-brief-import-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------------------------
// Swift's BriefImportTests

#[test]
fn json_flat_object_shape() {
    let json = br#"{
      "abc123:1024": "neon glow grid",
      "def456:2048": "soft pastel waves"
    }"#;
    let records = parse_json(json).unwrap();
    assert_eq!(records.len(), 2);
    let by = |id: &str| records.iter().find(|r| r.clip_id.as_deref() == Some(id)).unwrap();
    assert_eq!(by("abc123:1024").brief, "neon glow grid");
    assert_eq!(by("def456:2048").brief, "soft pastel waves");
    assert_eq!(by("abc123:1024").path, None);
    // Swift's order here is a hash order; the port keeps the file's.
    assert_eq!(ids(&records), [Some("abc123:1024"), Some("def456:2048")]);
}

#[test]
fn json_array_of_objects_shape() {
    let json = br#"[
      {"clip_id": "x:1", "brief": "first"},
      {"path": "/Users/me/clip.mov", "brief": "second"},
      {"clipID": "y:2", "current_path": "/y.mov", "brief": "third"}
    ]"#;
    assert_eq!(
        parse_json(json).unwrap(),
        [r(Some("x:1"), None, "first"), r(None, Some("/Users/me/clip.mov"), "second"), r(Some("y:2"), Some("/y.mov"), "third")]
    );
}

#[test]
fn json_briefs_wrapper_shape() {
    let json = br#"{"version": 1, "briefs": [{"clip_id": "wrap:1", "brief": "a"}, {"clip_id": "wrap:2", "brief": "b"}]}"#;
    assert_eq!(parse_json(json).unwrap(), [r(Some("wrap:1"), None, "a"), r(Some("wrap:2"), None, "b")]);
}

#[test]
fn json_skips_records_without_brief() {
    let json = br#"[{"clip_id": "ok:1", "brief": "yes"}, {"clip_id": "skip:1"}, {"clip_id": "skip:2", "brief": ""}, {"clip_id": "skip:3", "brief": "   "}]"#;
    assert_eq!(ids(&parse_json(json).unwrap()), [Some("ok:1")]);
}

#[test]
fn json_skips_records_without_clip_id_or_path() {
    let json = br#"[{"brief": "orphan"}, {"clip_id": "match:1", "brief": "kept"}]"#;
    assert_eq!(parse_json(json).unwrap(), [r(Some("match:1"), None, "kept")]);
}

#[test]
fn json_malformed_and_unexpected_top_level() {
    assert_eq!(parse_json(b"{ this isn't json"), malformed());
    assert_eq!(parse_json(br#""just a string""#), malformed());
}

#[test]
fn csv_header_and_rows() {
    let csv = "clip_id,current_path,brief\na:1,/tmp/a.mov,first brief\nb:2,/tmp/b.mov,second brief";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("a:1"), Some("/tmp/a.mov"), "first brief"), r(Some("b:2"), Some("/tmp/b.mov"), "second brief")]);
}

#[test]
fn csv_header_aliases_case_insensitive() {
    assert_eq!(parse_csv("ClipID,FilePath,Brief\nx:1,/X.mov,X brief").unwrap(), [r(Some("x:1"), Some("/X.mov"), "X brief")]);
}

#[test]
fn csv_ignores_unknown_columns() {
    let csv = "timestamp,clip_id,input_tokens,output_tokens,brief\n2026-05-23T15:17:35+00:00,a:1,7,144,user-added brief";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("a:1"), None, "user-added brief")]);
}

#[test]
fn csv_no_brief_column() {
    let csv = "timestamp,clip_id,input_tokens,output_tokens\n2026-05-23T15:17:35+00:00,a:1,7,144";
    assert_eq!(parse_csv(csv), Err(BriefImportError::CsvNoBriefColumn));
}

#[test]
fn csv_quoted_fields_with_commas() {
    let csv = "clip_id,brief\nq:1,\"brief with, embedded comma\"\nq:2,\"brief with \"\"quoted\"\" word\"";
    let records = parse_csv(csv).unwrap();
    assert_eq!(records[0].brief, "brief with, embedded comma");
    assert_eq!(records[1].brief, "brief with \"quoted\" word");
}

#[test]
fn csv_crlf_line_endings() {
    let records = parse_csv("clip_id,brief\r\na:1,winter scene\r\nb:2,summer scene\r\n").unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].brief, "summer scene");
}

#[test]
fn csv_skips_blank_brief_rows() {
    assert_eq!(ids(&parse_csv("clip_id,brief\nkeep:1,real brief\nskip:1,\nskip:2,").unwrap()), [Some("keep:1")]);
}

#[test]
fn csv_skips_rows_missing_matchable_id() {
    assert_eq!(ids(&parse_csv("clip_id,brief\n,orphan brief\nmatch:1,real brief").unwrap()), [Some("match:1")]);
}

#[test]
fn csv_empty_input() {
    assert_eq!(parse_csv(""), Err(BriefImportError::CsvMissingHeader));
}

#[test]
fn parse_dispatches_by_extension() {
    let dir = scratch("dispatch");
    std::fs::write(dir.join("briefs.json"), r#"{"a:1":"one"}"#).unwrap();
    assert_eq!(parse(&dir.join("briefs.json")).unwrap(), [r(Some("a:1"), None, "one")]);
    std::fs::write(dir.join("briefs.csv"), "clip_id,brief\nx:1,short\n").unwrap();
    assert_eq!(parse(&dir.join("briefs.csv")).unwrap(), [r(Some("x:1"), None, "short")]);
    std::fs::write(dir.join("briefs.txt"), "doesn't matter").unwrap();
    assert_eq!(parse(&dir.join("briefs.txt")), Err(BriefImportError::UnsupportedExtension("txt".into())));
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------------------------
// Edge cases, expected values from Swift's parser

#[test]
fn byte_order_marks() {
    // JSON: a UTF-8 BOM is skipped; UTF-16 (with or without a BOM) and BOM-less UTF-32 are read.
    assert_eq!(parse_json(b"\xEF\xBB\xBF{\"a:1\": \"bom brief\"}").unwrap(), [r(Some("a:1"), None, "bom brief")]);
    let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("{\"utf16\": \"sixteen\"}".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    assert_eq!(parse_json(&utf16).unwrap(), [r(Some("utf16"), None, "sixteen")]);
    let utf16be: Vec<u8> = "{\"a\":\"x\"}".encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert_eq!(parse_json(&utf16be).unwrap(), [r(Some("a"), None, "x")]);
    let utf32le: Vec<u8> = "{\"a\":\"x\"}".chars().flat_map(|c| (c as u32).to_le_bytes()).collect();
    assert_eq!(parse_json(&utf32le).unwrap(), [r(Some("a"), None, "x")]);
    // The UTF-32 little-endian BOM reads as UTF-16's, then a NUL: refused.
    let utf32bom: Vec<u8> = [0xFF, 0xFE, 0, 0].into_iter().chain(utf32le.iter().copied()).collect();
    assert_eq!(parse_json(&utf32bom), malformed());
    // A 4-byte UTF-16 document is not detected.
    assert_eq!(parse_json(&"[]".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>()), malformed());
    assert_eq!(parse_json(&"[ ]".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>()).unwrap(), []);
    // One leading U+FEFF of every string is dropped, written or escaped.
    assert_eq!(parse_json("{\"\u{FEFF}\u{FEFF}a\":\"x\"}".as_bytes()).unwrap(), [r(Some("\u{FEFF}a"), None, "x")]);
    assert_eq!(parse_json(br#"{"\ufeffa":"x"}"#).unwrap(), [r(Some("a"), None, "x")]);
    assert_eq!(parse_json("{\"a\u{FEFF}b\":\"x\"}".as_bytes()).unwrap(), [r(Some("a\u{FEFF}b"), None, "x")]);
    assert_eq!(parse_json("[{\"clip_id\":\"\u{FEFF}id\",\"brief\":\"\u{FEFF}\u{FEFF} two\"}]".as_bytes()).unwrap(), [r(Some("id"), None, "\u{FEFF} two")]);

    // CSV text: a BOM is part of the first header name, which then matches nothing.
    assert_eq!(parse_csv("\u{FEFF}clip_id,brief\nbom:1,bom brief\n").unwrap(), []);
    assert_eq!(parse_csv("\u{FEFF}brief,clip_id\nbom brief,bom:1\n"), Err(BriefImportError::CsvNoBriefColumn));
    // A CSV file: reading it as UTF-8 drops the BOM.
    let dir = scratch("bom");
    std::fs::write(dir.join("bom.csv"), b"\xEF\xBB\xBFclip_id,brief\nbom:1,bom brief\n").unwrap();
    assert_eq!(parse(&dir.join("bom.csv")).unwrap(), [r(Some("bom:1"), None, "bom brief")]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn line_endings_and_quoted_newlines() {
    let csv = "clip_id,brief\nq:1,\"line one\nline two\"\nq:2,\"crlf one\r\ncrlf two\"\r\nq:3,\"\n  padded\n\"\n";
    assert_eq!(
        parse_csv(csv).unwrap(),
        [r(Some("q:1"), None, "line one\nline two"), r(Some("q:2"), None, "crlf one\r\ncrlf two"), r(Some("q:3"), None, "padded")]
    );
    assert_eq!(parse_csv("clip_id,brief\ra:1,cr only\rb:2,cr two\r").unwrap(), [r(Some("a:1"), None, "cr only"), r(Some("b:2"), None, "cr two")]);
    assert_eq!(parse_csv("clip_id,brief\r\r\na:1,double cr\r\r\n\r\nb:2,end\n").unwrap(), [r(Some("a:1"), None, "double cr"), r(Some("b:2"), None, "end")]);
    assert_eq!(parse_csv_rows("a\r\rb\r\n"), [vec!["a".to_string()], vec!["".to_string()], vec!["b".to_string()]]);
    // An unterminated quote runs to the end.
    assert_eq!(parse_csv("clip_id,brief\nu:1,\"unterminated, quote\nnext,line").unwrap(), [r(Some("u:1"), None, "unterminated, quote\nnext,line")]);
    // JSON briefs keep inner newlines and lose the outer ones.
    assert_eq!(parse_json(br#"{"a": "\n\nline one\nline two\r\n\n"}"#).unwrap(), [r(Some("a"), None, "line one\nline two")]);
}

#[test]
fn quoting_edges() {
    let csv = "clip_id,brief\nm:1,a\"b\"c\nm:2,\"ab\"cd\nm:3,\"a\"\"b\"\nm:4,\"\"\nm:5,\"\"\"x\"\"\"\n";
    assert_eq!(
        parse_csv(csv).unwrap(),
        [r(Some("m:1"), None, "a\"b\"c"), r(Some("m:2"), None, "abcd"), r(Some("m:3"), None, "a\"b"), r(Some("m:5"), None, "\"x\"")]
    );
    let csv = "clip_id,brief\nr:1,\"\",x\nr:2,\"\"\"\",y\n\"r:3\",\"q\"\"uote\"\n\"\",orphan\n";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("r:2"), None, "\""), r(Some("r:3"), None, "q\"uote")]);
    assert_eq!(parse_csv("clip_id,brief\nw:1,\"quoted\"  ,trailing\nw:2,\"a\" b\n").unwrap(), [r(Some("w:1"), None, "quoted"), r(Some("w:2"), None, "a b")]);
    assert_eq!(parse_csv("clip_id,\"brief\"\nqh:1,quoted header\n").unwrap(), [r(Some("qh:1"), None, "quoted header")]);
}

#[test]
fn duplicate_ids_and_keys() {
    // CSV: every row is a record, duplicates included.
    let csv = "clip_id,brief\ndup:1,first\ndup:1,second\ndup:1,first\n";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("dup:1"), None, "first"), r(Some("dup:1"), None, "second"), r(Some("dup:1"), None, "first")]);
    // JSON: a key given twice keeps its first value.
    assert_eq!(parse_json(br#"{"a": "one", "a": "two", "b": "x"}"#).unwrap(), [r(Some("a"), None, "one"), r(Some("b"), None, "x")]);
    assert_eq!(
        parse_json(br#"[{"clip_id": "a", "clip_id": "b", "brief": "x"}, {"brief": "y", "brief": "z", "path": "/p"}]"#).unwrap(),
        [r(Some("a"), None, "x"), r(None, Some("/p"), "y")]
    );
    // CSV header: the first column of each name wins.
    let csv = "brief,clip_id,brief,clipid,path,filepath\nfirst,d:1,second,d:2,/p1,/p2\n";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("d:1"), Some("/p1"), "first")]);
}

#[test]
fn empty_briefs_and_ids() {
    assert_eq!(parse_csv("clip_id,brief\ne:1,\ne:2,\"\"\ne:3,\"   \"\ne:4\n,\n\n\n").unwrap(), []);
    assert_eq!(parse_csv("clip_id,brief\n  ,  \n   ,brief no id\n").unwrap(), []);
    assert_eq!(parse_csv("clip_id,brief\n"), Ok(vec![]));
    assert_eq!(parse_csv("\n"), Err(BriefImportError::CsvNoBriefColumn));
    assert_eq!(parse_csv(",\n"), Err(BriefImportError::CsvNoBriefColumn));
    // JSON: an empty or blank clip_id / path is still an id (only a missing or non-string one is not).
    assert_eq!(
        parse_json(br#"[{"clip_id": "", "brief": "empty id"}, {"path": "", "brief": "empty path"}, {"clip_id": "  ", "brief": "ws id"}]"#).unwrap(),
        [r(Some(""), None, "empty id"), r(None, Some(""), "empty path"), r(Some("  "), None, "ws id")]
    );
    let json = br#"[{"clip_id": 5, "path": "/p", "brief": "num id"}, {"clip_id": null, "brief": "null id"}, {"clip_id": "c", "brief": 7},
        {"clip_id": "d", "brief": null}, {"clip_id": true, "brief": "bool id"}, {"path": ["/x"], "brief": "array path"}]"#;
    assert_eq!(parse_json(json).unwrap(), [r(None, Some("/p"), "num id")]);
    assert_eq!(parse_json(br#"[{"clip_id": "z", "brief": "  padded  ", "path": "  /p  "}]"#).unwrap(), [r(Some("z"), Some("  /p  "), "padded")]);
    assert_eq!(parse_json(b"[]").unwrap(), []);
    assert_eq!(parse_json(b"{}").unwrap(), []);
    assert_eq!(parse_json(br#"{"briefs": [], "a": "y"}"#).unwrap(), []);
    assert_eq!(parse_json(br#"{"briefs": "x", "a": "y"}"#).unwrap(), [r(Some("briefs"), None, "x"), r(Some("a"), None, "y")]);
    assert_eq!(parse_json(br#"{"briefs": [1, "s", null, [], {"clip_id": "a", "brief": "b"}]}"#).unwrap(), [r(Some("a"), None, "b")]);
}

#[test]
fn trimming_is_foundations() {
    // Tab, CR, LF, NBSP, U+3000, U+2028/9, U+0085 and the zero-width space trim; U+FEFF does not.
    let json = "{\"tab\": \"\\t\\n brief \\r\\n\", \"nbsp\": \"\u{a0}nb\u{a0}\", \"ideo\": \"\u{3000}wide\u{3000}\", \"zwsp\": \"\u{200b}z\u{200b}\", \"ls\": \"\u{2028}x\u{2029}\", \"nel\": \"\u{85}y\u{85}\", \"onlyws\": \" \\t\u{a0}\u{3000}\"}";
    assert_eq!(
        parse_json(json.as_bytes()).unwrap(),
        [
            r(Some("tab"), None, "brief"),
            r(Some("nbsp"), None, "nb"),
            r(Some("ideo"), None, "wide"),
            r(Some("zwsp"), None, "z"),
            r(Some("ls"), None, "x"),
            r(Some("nel"), None, "y")
        ]
    );
    let csv = "clip_id,brief\n\u{a0}nb:1\u{a0},\u{3000}wide brief\u{3000}\n\u{200b}z:1,zw\u{200b}\n";
    assert_eq!(parse_csv(csv).unwrap(), [r(Some("nb:1"), None, "wide brief"), r(Some("z:1"), None, "zw")]);
    assert_eq!(parse_csv(" Clip_ID , BRIEF ,  File_Path \nh:1,  spaced brief  ,  /p.mov  \n").unwrap(), [r(Some("h:1"), Some("/p.mov"), "spaced brief")]);
}

#[test]
fn json_reading_rules() {
    // Accepted: one trailing comma, big integers, underflow, `\/`, U+007F, an escaped NUL, trailing whitespace.
    for ok in [
        &br#"[{"clip_id":"a","brief":"x"},]"#[..],
        br#"{"a":"x", }"#,
        b"{\"a\":\"x\",\n}",
        br#"{"n":[1,2,],"a":"x"}"#,
        br#"{"n":18446744073709551616,"a":"x"}"#,
        br#"{"n":123456789012345678901234567890,"a":"x"}"#,
        br#"{"n":-1.0e-400,"a":"x"}"#,
        br#"{"n":1.7976931348623157e308,"a":"x"}"#,
        br#"{"n":-0,"a":"x"}"#,
        br#"{"n":1E5,"a":"x"}"#,
        b"\t\r\n {\"a\":\"x\"} \n\n",
    ] {
        assert_eq!(parse_json(ok).unwrap().last().map(|r| r.brief.as_str()), Some("x"), "{}", String::from_utf8_lossy(ok));
    }
    assert_eq!(parse_json(br#"{"a":"esc\/ok"}"#).unwrap(), [r(Some("a"), None, "esc/ok")]);
    assert_eq!(parse_json(br#"{"a":"nul\u0000inside"}"#).unwrap(), [r(Some("a"), None, "nul\0inside")]);
    assert_eq!(parse_json(b"{\"a\":\"\x7f del\"}").unwrap(), [r(Some("a"), None, "\x7f del")]);
    assert_eq!(parse_json(br#"{"e": "caf\u00e9", "f": "\ud83d\ude00 face"}"#).unwrap(), [r(Some("e"), None, "café"), r(Some("f"), None, "😀 face")]);
    // Refused, as Swift refuses them.
    for bad in [
        &b""[..],
        b"   \n  ",
        b"5",
        b"null",
        b"[,]",
        b"{,}",
        br#"{"a":"x",,}"#,
        b"{'a':'x'}",
        br#"{"a":NaN}"#,
        br#"{"a":01}"#,
        br#"{"a":+1}"#,
        br#"{"a":1.}"#,
        br#"{"a":.5}"#,
        br#"{"a":1e}"#,
        br#"{"a":1e400,"b":"x"}"#,
        br#"{"a":1.8e308,"b":"x"}"#,
        b"{a:\"x\"}",
        b"{\"a\":\"tab\there\"}",
        b"{\"a\":\"nl\nhere\"}",
        b"{\"a\":\"\x01 ctl\"}",
        br#"{"a":"bad\xesc"}"#,
        br#"{"a":"\uD83D"}"#,
        br#"{"a":"\udc00 x"}"#,
        br#"{"a":"x","b":}"#,
        br#"{"a":"x"}{"b":"y"}"#,
        br#"{"a": "x"} trailing"#,
        br#"{"a":"x"}//c"#,
        br#"{"a":"x" /* c */}"#,
        b"\x00{\"a\":\"x\"}",
        b"{\"a\":\"x\"}\x00",
        b"{\"a\":\"\xff bad utf8\"}",
        b"{\"a\":\"\xc0\xaf\"}",
        b"{\"a\":\"\xed\xa0\x80\"}",
        b"\xEF\xBB\xBF",
        b"[",
        b"{",
    ] {
        assert_eq!(parse_json(bad), malformed(), "{}", String::from_utf8_lossy(bad));
    }
    // Swift's NFC / NFD keys: its dictionary merges them; here they stay two records (module notes).
    assert_eq!(parse_json("{\"caf\u{e9}\": \"nfc\", \"cafe\u{301}\": \"nfd\"}".as_bytes()).unwrap().len(), 2);
}

#[test]
fn nesting_limit() {
    let nested = |levels: usize, inner: &str| format!("{}{inner}{}", "[".repeat(levels), "]".repeat(levels));
    // 512 containers deep may hold values; a 513th level may only be empty.
    assert_eq!(parse_json(nested(511, r#"{"clip_id":"a","brief":"b"}"#).as_bytes()).unwrap(), []);
    assert_eq!(parse_json(nested(512, r#"{"clip_id":"a","brief":"b"}"#).as_bytes()), malformed());
    assert_eq!(parse_json(nested(512, "{ }").as_bytes()).unwrap(), []);
    let in_object = |levels: usize| format!(r#"{{"x":{},"b":"y"}}"#, nested(levels, ""));
    assert_eq!(parse_json(in_object(512).as_bytes()).unwrap(), [r(Some("b"), None, "y")]);
    assert_eq!(parse_json(in_object(513).as_bytes()), malformed());
    // Deep input fails without deep recursion.
    assert_eq!(parse_json(nested(100_000, "").as_bytes()), malformed());
}

#[test]
fn csv_rows_shapes() {
    assert_eq!(
        parse_csv("clip_id,path,brief\ns:1\ns:2,/p\ns:3,/p,ok\n,/only-path,path brief\n").unwrap(),
        [r(Some("s:3"), Some("/p"), "ok"), r(None, Some("/only-path"), "path brief")]
    );
    assert_eq!(parse_csv("clip_id,brief,\nx:1,extra,cols,here,\n").unwrap(), [r(Some("x:1"), None, "extra")]);
    assert_eq!(parse_csv("clip_id,brief\nlast:1,no newline at end").unwrap(), [r(Some("last:1"), None, "no newline at end")]);
    assert_eq!(parse_csv("path,brief\n/a.mov,path only\n").unwrap(), [r(None, Some("/a.mov"), "path only")]);
    assert_eq!(parse_csv("clip_id;brief\nsemi:1;semicolon\n"), Err(BriefImportError::CsvNoBriefColumn));
    assert_eq!(parse_csv_rows(""), Vec::<Vec<String>>::new());
    assert_eq!(parse_csv_rows("a,"), [vec!["a".to_string(), String::new()]]);
}

#[test]
fn files_extensions_and_unreadable() {
    let dir = scratch("files");
    let flat = br#"{"abc123:1024": "neon glow grid"}"#;
    for (name, ok) in [("up.JSON", true), (".json", false), ("noext", false), ("trail.json.", false), ("a.b.json", true)] {
        std::fs::write(dir.join(name), flat).unwrap();
        let got = parse(&dir.join(name));
        if ok {
            assert_eq!(got.unwrap().len(), 1, "{name}");
        } else {
            assert_eq!(got, Err(BriefImportError::UnsupportedExtension(String::new())), "{name}");
        }
    }
    std::fs::write(dir.join("mixed.Csv"), "clip_id,brief\r\na:1,winter scene\r\n").unwrap();
    assert_eq!(parse(&dir.join("mixed.Csv")).unwrap(), [r(Some("a:1"), None, "winter scene")]);
    std::fs::write(dir.join("bad.csv"), b"clip_id,brief\nbad:1,caf\xe9 latin1\n").unwrap();
    assert_eq!(parse(&dir.join("bad.csv")), Err(BriefImportError::Unreadable("The file couldn’t be opened because it isn’t in the correct format.".into())));
    assert_eq!(
        parse(&dir.join("missing.json")),
        Err(BriefImportError::Unreadable("The file “missing.json” couldn’t be opened because there is no such file.".into()))
    );
    std::fs::create_dir(dir.join("dir.csv")).unwrap();
    assert_eq!(parse(&dir.join("dir.csv")), Err(BriefImportError::Unreadable("The file “dir.csv” couldn’t be opened.".into())));
    std::fs::write(dir.join("empty.csv"), b"").unwrap();
    assert_eq!(parse(&dir.join("empty.csv")), Err(BriefImportError::CsvMissingHeader));
    let _ = std::fs::remove_dir_all(dir);
}
