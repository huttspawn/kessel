//! event_code -- test whether an achievement event code is a hash of the event
//! name a hydra "Achievement Event" action carries.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example event_code -- <hex code> <name> [names...]

use kessel::hash;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let code = u64::from_str_radix(
        args.get(1)
            .ok_or_else(|| anyhow::anyhow!("usage: <hex code> <name>..."))?,
        16,
    )?;
    let low48 = code & 0xFFFF_FFFF_FFFF;
    for name in &args[2..] {
        for form in [name.clone(), name.to_lowercase(), name.to_uppercase()] {
            let (ph, sh) = hash::hashlittle2(form.as_bytes(), 0, 0);
            let combined = hash::combine_hash(ph, sh);
            let swapped = (u64::from(sh) << 32) | u64::from(ph);
            for (label, v) in [("ph<<32|sh", combined), ("sh<<32|ph", swapped)] {
                let hit = v & 0xFFFF_FFFF_FFFF == low48 || v == code;
                println!("{form:<40} {label}: {v:016X}{}", if hit { "  MATCH" } else { "" });
            }
        }
    }
    Ok(())
}
