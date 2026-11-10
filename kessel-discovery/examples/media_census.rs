//! media_census -- decode push 3 (#429), media group: find every model,
//! animation and appearance-asset file in the archives by CONTENT, with no
//! hash dictionary (FR-KESSEL-024). The dictionary, when given, is a
//! cross-check only: it reports which content class each dictionary
//! extension lands in, never the other way round.
//!
//!   cargo run --release -p kessel-discovery --example media_census -- \
//!       <assets dir> <out dir> [hashes_filename.txt|-] [samples per class] [classes to dump whole]
//!
//! Writes <out>/census.tsv (archive, hash, size, class, first 16 bytes hex,
//! dictionary path or '-') and up to N sample files per class under
//! <out>/samples/<class>/<hash>.bin. Prints per-class counts and, with the
//! dictionary, a class x extension table.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;

use kessel::hash::HashDictionary;
use kessel::myp::Archive;

/// Content class from the first bytes. Every rule is a byte test; no path.
fn classify(d: &[u8]) -> String {
    let m4 = |m: &[u8]| d.len() >= m.len() && &d[..m.len()] == m;
    if m4(b"GAWB") {
        return "GAWB".into(); // granny-derived model (.gr2)
    }
    if m4(b"JAWB") {
        return "JAWB".into(); // animation v2 (.jba)
    }
    if m4(b"AMX ") {
        return "AMX".into(); // animation cue metadata (.amx)
    }
    if m4(b"DDS ") {
        return "DDS".into();
    }
    if m4(b"PBUK") || m4(b"DBLB") || m4(b"PROT") || m4(b"SCPT") {
        return "GOM".into();
    }
    if m4(b"MAG\0") || m4(b"MAG ") {
        return "MAG".into();
    }
    if m4(&[0xFF, 0xFE]) {
        // UTF-16-LE: name the root element
        let text: String = d[2..d.len().min(400)]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 && c[1] == 0 {
                    Some(c[0] as char)
                } else {
                    None
                }
            })
            .collect();
        return format!("U16:{}", root_tag(&text));
    }
    if m4(b"<") || m4(&[0xEF, 0xBB, 0xBF, b'<']) {
        let text = String::from_utf8_lossy(&d[..d.len().min(400)]).to_string();
        return format!("XML:{}", root_tag(&text));
    }
    if m4(b"RIFF") {
        return "RIFF".into();
    }
    if m4(b"BKHD") {
        return "BKHD".into(); // Wwise sound bank
    }
    if d.len() >= 4 && d[..4].iter().all(|b| b.is_ascii_alphanumeric() || *b == b' ') {
        return format!("M4:{}", String::from_utf8_lossy(&d[..4]));
    }
    if d.len() >= 4 {
        return format!("HEX:{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3]);
    }
    "SHORT".into()
}

/// First element name after '<', skipping a '<?xml ...?>' declaration.
fn root_tag(text: &str) -> String {
    let mut rest = text;
    loop {
        let Some(i) = rest.find('<') else {
            return "?".into();
        };
        rest = &rest[i + 1..];
        if rest.starts_with('?') || rest.starts_with('!') {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        return if name.is_empty() { "?".into() } else { name };
    }
}

fn ext_of(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    match file.split_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => "(none)".into(),
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        anyhow::bail!("usage: media_census <assets dir> <out dir> [dict] [samples]");
    }
    let assets = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    let mut dict = HashDictionary::new();
    let have_dict = match args.get(3) {
        Some(p) if p != "-" => {
            let n = dict.load(p)?;
            eprintln!("cross-check dictionary: {n} paths");
            true
        }
        _ => false,
    };
    let samples: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(40);
    // classes written in full (comma list, e.g. AMX,JAWB), past the sample cap
    let whole: Vec<String> = args.get(5).map(|s| s.split(',').map(String::from).collect()).unwrap_or_default();
    std::fs::create_dir_all(out.join("samples"))?;
    let mut tsv = std::io::BufWriter::new(std::fs::File::create(out.join("census.tsv"))?);

    let mut tors: Vec<PathBuf> = std::fs::read_dir(&assets)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "tor"))
        .collect();
    tors.sort();

    let mut class_n: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut class_ext: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut sampled: HashMap<String, usize> = HashMap::new();
    let mut errors = 0usize;
    for (i, tor) in tors.iter().enumerate() {
        let name = tor.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut ar = match Archive::open(tor) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("skip {name}: {e}");
                continue;
            }
        };
        let entries: Vec<_> = ar.entries()?.cloned().collect();
        for e in &entries {
            let d = match ar.read_entry(e) {
                Ok(d) => d,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let class = classify(&d);
            let slot = class_n.entry(class.clone()).or_insert((0, 0));
            slot.0 += 1;
            slot.1 += d.len() as u64;
            let dpath = if have_dict { dict.get(e.filename_hash).cloned() } else { None };
            if have_dict {
                let ext = dpath.as_deref().map(ext_of).unwrap_or_else(|| "(unnamed)".into());
                *class_ext.entry((class.clone(), ext)).or_insert(0) += 1;
            }
            let head: String = d.iter().take(16).map(|b| format!("{b:02x}")).collect();
            writeln!(
                tsv,
                "{name}\t{:016X}\t{}\t{class}\t{head}\t{}",
                e.filename_hash,
                d.len(),
                dpath.as_deref().unwrap_or("-")
            )?;
            let k = sampled.entry(class.clone()).or_insert(0);
            if *k < samples || whole.contains(&class) {
                *k += 1;
                let dir = out.join("samples").join(class.replace([':', '/'], "_"));
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join(format!("{:016X}.bin", e.filename_hash)), &d)?;
            }
        }
        eprintln!("[{}/{}] {name}: {} entries", i + 1, tors.len(), entries.len());
    }
    tsv.flush()?;

    println!("read errors: {errors}");
    println!("{:<34} {:>9} {:>14}", "class", "files", "bytes");
    for (c, (n, b)) in &class_n {
        println!("{c:<34} {n:>9} {b:>14}");
    }
    if have_dict {
        println!("\nclass x dictionary extension (cross-check only)");
        for ((c, x), n) in &class_ext {
            println!("{c:<34} {x:<20} {n:>9}");
        }
    }
    Ok(())
}
