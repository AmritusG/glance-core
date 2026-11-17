//! The Rust side of the files cross-test: the same modes, input files and output lines as Swift MaCrate's
//! FilesOracle (tools/swift-library-oracle/FilesOracle.swift), computed by glance-library, so the two outputs are
//! compared with `diff`. tools/swift-library-oracle/files-crosstest.sh runs both.
//!
//!   cargo run --release -p glance-library --example files_twin -- <mode> <arg> …
#![allow(clippy::print_stdout)]

use glance_library::enumerate::{direct_videos, foundation_mtime, localized_case_insensitive_cmp, recursive_videos};
use glance_library::header::{HeaderInfo, container_info, extract_info};
use glance_library::roots::{canonical_path, is_contained, is_dangerous_root, last_path_component, standardized_path};
use glance_library::thumbkey::{downscaled_size, swift_double, thumbnail_key, thumbnail_key_for};
use glance_library::{clip_id, roots};
use std::collections::HashSet;
use std::path::Path;

#[path = "../tests/files_support/tree.rs"]
mod tree;

#[path = "../tests/files_support/headers.rs"]
mod headers;

fn bits(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_string(), |d| format!("{:016x}", d.to_bits()))
}

fn info(i: &HeaderInfo) -> String {
    let (w, h) = i.size.map_or(("-".to_string(), "-".to_string()), |(w, h)| (w.to_string(), h.to_string()));
    format!("{}\t{}\t{w}\t{h}", bits(i.duration), bits(i.fps))
}

fn lines(path: &str) -> Vec<String> {
    std::fs::read_to_string(path).unwrap_or_default().split('\n').filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// Every non-directory entry under `dir`, byte order of the relative path (as the oracle).
fn files(dir: &str) {
    let mut rel = Vec::new();
    let mut stack = vec![(Path::new(dir).to_path_buf(), String::new())];
    while let Some((d, prefix)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push((e.path(), r.clone())),
                _ => {}
            }
            rel.push(r);
        }
    }
    rel.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    for r in rel {
        let p = format!("{dir}/{r}");
        let path = Path::new(&p);
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            println!("{r}\tgone");
            continue;
        };
        if meta.is_dir() {
            continue;
        }
        let kind = if meta.file_type().is_symlink() {
            "link"
        } else if meta.is_file() {
            "file"
        } else {
            "other"
        };
        let id = clip_id::clip_id(path).unwrap_or_else(|_| "-".into());
        let key = thumbnail_key_for(path).unwrap_or_else(|| "-".into());
        #[cfg(unix)]
        let (sec, nsec) = {
            use std::os::unix::fs::MetadataExt;
            (meta.mtime(), meta.mtime_nsec())
        };
        #[cfg(not(unix))]
        let (sec, nsec) = (0i64, 0i64);
        let mtime = bits(foundation_mtime(&meta));
        println!("{r}\t{kind}\t{id}\t{}\t{}\t{key}\t{sec}\t{nsec}\t{mtime}\t{mtime}\t{}", info(&container_info(path)), info(&extract_info(path)), meta.len());
    }
}

fn enumerate(root: &str, excluded: &[String]) {
    match direct_videos(Path::new(root)) {
        Ok(clips) => {
            for c in clips {
                println!("direct\t{}\t{}\t{}\t{}", c.path, c.size, bits(Some(c.mtime)), thumbnail_key(&c.path, c.size, c.mtime));
            }
        }
        Err(_) => println!("direct-error"),
    }
    let set: HashSet<String> = excluded.iter().map(|e| standardized_path(e)).collect();
    for c in recursive_videos(Path::new(root), &set, None, &|| false) {
        println!("recursive\t{}\t{}\t{}\t{}", c.path, c.size, bits(Some(c.mtime)), thumbnail_key(&c.path, c.size, c.mtime));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(mode), Some(arg)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: files_twin files|tree|headers|enum|doubles|compare|sort|paths|contained|clipsize|downscale <arg> …");
        std::process::exit(2);
    };
    match mode.as_str() {
        "files" => files(arg),
        // Write the header cases into `arg`.
        "headers" => {
            if let Err(e) = headers::write(Path::new(arg)) {
                eprintln!("headers: {e}");
                std::process::exit(1);
            }
        }
        // Build the synthetic cross-test folder at `arg` (must not exist).
        "tree" => {
            if let Err(e) = tree::build(Path::new(arg)) {
                eprintln!("tree: {e}");
                std::process::exit(1);
            }
        }
        "enum" => enumerate(arg, args.get(3..).unwrap_or_default()),
        "doubles" => {
            for l in lines(arg) {
                println!("{}", swift_double(f64::from_bits(u64::from_str_radix(&l, 16).unwrap_or(0))));
            }
        }
        "compare" => {
            for l in lines(arg) {
                match l.split_once('\t') {
                    Some((a, b)) => println!("{}", localized_case_insensitive_cmp(a, b) as i8),
                    None => println!("bad"),
                }
            }
        }
        "sort" => {
            let mut names = lines(arg);
            names.sort_by(|a, b| localized_case_insensitive_cmp(a, b));
            for n in names {
                println!("{n}");
            }
        }
        "paths" => {
            for p in lines(arg) {
                let std = standardized_path(&p);
                println!("{std}\t{}\t{}\t{}", canonical_path(&p), is_dangerous_root(&std), last_path_component(&p));
            }
        }
        "contained" => {
            for l in lines(arg) {
                match l.split_once('\t') {
                    Some((c, r)) => println!("{}", is_contained(c, r)),
                    None => println!("bad"),
                }
            }
        }
        "clipsize" => {
            for l in lines(arg) {
                println!("{}", clip_id::size_from_clip_id(&l).map_or_else(|| "-".to_string(), |s| s.to_string()));
            }
        }
        "downscale" => {
            for l in lines(arg) {
                let v: Vec<u32> = l.split(' ').filter_map(|x| x.parse().ok()).collect();
                match v.as_slice() {
                    [w, h] => {
                        let (a, b) = downscaled_size(*w, *h);
                        println!("{a} {b}");
                    }
                    _ => println!("bad"),
                }
            }
        }
        "json" => {
            // Swift's library-roots.json lines (FilesOracle `json` output) read back and written again.
            for l in lines(arg) {
                let Some(roots) = roots::decode_library_roots(l.as_bytes()) else {
                    println!("bad");
                    continue;
                };
                println!("{}", roots::encode_library_roots(&roots).unwrap_or_else(|| "unencodable".into()));
            }
        }
        _ => {
            eprintln!("unknown mode {mode}");
            std::process::exit(2);
        }
    }
}
