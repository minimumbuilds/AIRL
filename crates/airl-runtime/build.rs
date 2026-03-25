use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Find libairl_rt.a from the cargo build output and compress it for embedding.
    // This allows `airl compile` to work without the library on disk.
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Search for the library in typical cargo output locations
    let candidates = [
        // When built from workspace root
        PathBuf::from(&out_dir).join("../../../libairl_rt.a"),
        // Normalized paths
        PathBuf::from("target/release/libairl_rt.a"),
        PathBuf::from("target/debug/libairl_rt.a"),
        PathBuf::from("../target/release/libairl_rt.a"),
        PathBuf::from("../target/debug/libairl_rt.a"),
        PathBuf::from("../../target/release/libairl_rt.a"),
        PathBuf::from("../../target/debug/libairl_rt.a"),
    ];

    let mut found = None;
    for c in &candidates {
        if let Ok(canon) = c.canonicalize() {
            if canon.exists() {
                found = Some(canon);
                break;
            }
        }
    }

    let dest = PathBuf::from(&out_dir).join("libairl_rt.a.gz");

    if let Some(lib_path) = found {
        // Compress with gzip for embedding (~48MB -> ~13MB)
        use std::io::{Read, Write};
        let mut data = Vec::new();
        std::fs::File::open(&lib_path)
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&data).unwrap();
        let compressed = encoder.finish().unwrap();

        std::fs::write(&dest, &compressed).unwrap();
        println!(
            "cargo:warning=Embedded libairl_rt.a: {} -> {} bytes (compressed)",
            data.len(),
            compressed.len()
        );
    } else {
        // Write empty file so include_bytes! doesn't fail
        // The runtime will fall back to find_lib() at link time
        std::fs::write(&dest, &[]).unwrap();
        println!("cargo:warning=libairl_rt.a not found — AOT compile will search at link time");
    }
}
