//! Headless check of the native sound path: every `Sounds`-block resource
//! id from robots.dat is resolved through the archive table and decoded.
//!
//!   cargo run --example native_sound_check -- <Sound.pkg> [voices.pkg ...]

use matrixgame_rs::matrix_game::config::SoundDefs;
use matrixgame_rs::matrix_lib::base::pack::PkgArchive;
use matrixgame_rs::matrix_lib::base::storage::Storage;
use matrixgame_rs::platform::audio_native::decode_from_pkgs;

fn main() {
    let pkgs: Vec<PkgArchive> = std::env::args()
        .skip(1)
        .map(|p| PkgArchive::from_bytes(std::fs::read(&p).expect("read pkg")).expect("parse pkg"))
        .collect();
    let dat = std::fs::read("Data/robots.dat").expect("Data/robots.dat");
    let defs = SoundDefs::from_matrix_data(&Storage::from_bytes(&dat).expect("parse robots.dat"));
    let mut paths: Vec<String> = defs
        .iter()
        .filter(|(_, d)| !d.path.is_empty())
        .map(|(_, d)| d.path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    let mut failed = 0;
    for path in &paths {
        match decode_from_pkgs(&pkgs, path) {
            Ok((ch, rate, frames)) => println!("OK   {path}: {ch} ch, {rate} Hz, {frames} frames"),
            Err(e) => {
                failed += 1;
                println!("FAIL {path}: {e}");
            }
        }
    }
    println!("\n{} ids, {failed} failed", paths.len());
}
