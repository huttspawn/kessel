//! bucket_names -- what `pbuk::parse` returns for one bucket file: each object's
//! name, its offset-free header GUID and its payload size.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example bucket_names -- <file.bkt>

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <file.bkt>"))?;
    let data = std::fs::read(path)?;
    for obj in kessel::pbuk::parse(&data)? {
        let guid = obj
            .header
            .get(..8)
            .and_then(|b| <[u8; 8]>::try_from(b).ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0);
        println!("{guid:016x}\t{}\t{}", obj.fqn, obj.payload.len());
    }
    Ok(())
}
