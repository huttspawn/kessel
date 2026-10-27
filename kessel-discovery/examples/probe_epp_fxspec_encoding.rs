//! Probe for #294: classify the leading bytes of every dict-named `.epp` and
//! `.fxspec` entry in the archives, and count which ones kessel's decoders
//! (`decode_epp`, `decode_fxspec`) accept. Output feeds
//! `docs/probes/epp-fxspec-encoding.md`.
//!
//! cargo run --release -p kessel-discovery --example probe_epp_fxspec_encoding -- \
//!     -i ~/swtor/Assets -H ~/swtor/data/hashes_filename.txt

use anyhow::Result;
use clap::Parser;
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use kessel::schema::{epp, fxspec};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short = 'H', long)]
    hashes: PathBuf,
}

fn encoding(data: &[u8]) -> String {
    match data {
        [0xFF, 0xFE, ..] => "utf16le-bom".into(),
        [0xEF, 0xBB, 0xBF, ..] => "utf8-bom".into(),
        [b'<', 0x00, ..] => "utf16le-nobom".into(),
        [b'<', ..] => "utf8".into(),
        _ => format!("other:{}", hex::encode(&data[..data.len().min(8)])),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut dict = HashDictionary::new();
    dict.load(&args.hashes)?;
    let mut ext_by_hash: HashMap<u64, &'static str> = HashMap::new();
    for ext in [".epp", ".fxspec"] {
        for (h, p) in dict.paths_matching(ext) {
            if p.ends_with(ext) {
                ext_by_hash.insert(h, ext);
            }
        }
    }

    // (ext, encoding) -> (entries, decoded by kessel, one sample hash)
    let mut tally: BTreeMap<(&str, String), (u64, u64, u64)> = BTreeMap::new();
    for e in std::fs::read_dir(&args.input)? {
        let path = e?.path();
        if path.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let Ok(mut archive) = Archive::open(&path) else {
            continue;
        };
        let Ok(entries) = archive.entries().map(|e| e.cloned().collect::<Vec<_>>()) else {
            continue;
        };
        for entry in &entries {
            let Some(&ext) = ext_by_hash.get(&entry.filename_hash) else {
                continue;
            };
            let Ok(data) = archive.read_entry(entry) else {
                continue;
            };
            let decoded = match ext {
                ".epp" => epp::decode_epp(&data).is_ok(),
                _ => fxspec::decode_fxspec(&data, String::new())
                    .is_ok_and(|s| !s.node_classes.is_empty()),
            };
            let row = tally
                .entry((ext, encoding(&data)))
                .or_insert((0, 0, entry.filename_hash));
            row.0 += 1;
            row.1 += u64::from(decoded);
            if !decoded {
                let path = dict.get(entry.filename_hash).map_or("?", String::as_str);
                eprintln!("undecoded\t{path}");
            }
        }
    }

    println!("ext\tencoding\tentries\tdecoded\tsample_path");
    for ((ext, enc), (n, ok, sample)) in &tally {
        let sample = dict.get(*sample).map_or("?", String::as_str);
        println!("{ext}\t{enc}\t{n}\t{ok}\t{sample}");
    }
    Ok(())
}
