//! Pack audio files into `assets/sounds.bundle` for the WebAudio
//! backend. SR2 assets are proprietary, so the
//! payloads come from the user's SR2 installation:
//!
//!   cargo run --example pack_sounds -- <Sound.pkg | dir> [out.bundle]
//!
//! **Pkg mode** (arg ends in .pkg): reads SR2's `DATA/Sound.pkg` and
//! resolves each `Sound.X` resource id through the hand-derived
//! [`PKG_MAP`] table (the id→file tree lives in the host's encrypted
//! Main.dat; the SOUND/ROBOTS/* names map 1:1 so the table is short).
//!
//! **Folder mode**: for every `Sounds/<key>` entry in robots.dat with
//! a `path` (`Sound.WeapPlasma` style), the folder is probed for
//!   `<dir>/<key>.{wav,ogg,mp3}`          e.g. wplasma.ogg
//!   `<dir>/<path>.{wav,ogg,mp3}`         e.g. Sound.WeapPlasma.ogg
//!   `<dir>/<path with . as />.{wav,ogg,mp3}`  e.g. Sound/WeapPlasma.ogg
//! (case-insensitive on the stem).
//!
//! Matches are stored under the SR2 resource path — the key the
//! runtime sample cache uses. Missing entries are reported and stay
//! silent in-game (the spoken voice lines are NOT in Sound.pkg; they
//! ship with the SR2 host's speech resources).

use matrixgame_rs::gfx::bundle::AssetBundle;
use matrixgame_rs::matrix_game::config::SoundDefs;
use matrixgame_rs::matrix_lib::base::pack::PkgArchive;
use matrixgame_rs::matrix_lib::base::storage::Storage;
use std::collections::HashMap;
use std::path::Path;

const EXTS: [&str; 3] = ["wav", "ogg", "mp3"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: cargo run --example pack_sounds -- <Sound.pkg ...|dir> [out.bundle]"
        );
        std::process::exit(2);
    }
    let (pkg_paths, rest): (Vec<&String>, Vec<&String>) =
        args.iter().partition(|a| a.to_lowercase().ends_with(".pkg"));
    let dir = rest.first().filter(|a| !a.ends_with(".bundle")).cloned();
    let out = args
        .iter()
        .find(|a| a.ends_with(".bundle"))
        .cloned()
        .unwrap_or_else(|| "assets/sounds.bundle".into());

    let dat = std::fs::read("Data/robots.dat").expect("Data/robots.dat");
    let stor = Storage::from_bytes(&dat).expect("parse robots.dat");
    let defs = SoundDefs::from_matrix_data(&stor);

    let pkgs: Vec<PkgArchive> = pkg_paths
        .iter()
        .map(|p| {
            let data = std::fs::read(p).unwrap_or_else(|e| panic!("read {p}: {e}"));
            PkgArchive::from_bytes(data).unwrap_or_else(|e| panic!("parse {p}: {e}"))
        })
        .collect();
    for (p, pkg) in pkg_paths.iter().zip(&pkgs) {
        println!("{p}: {} files", pkg.list_files().len());
    }

    // Folder mode: index the folder once (lowercased relative stem → path).
    let mut files: HashMap<String, std::path::PathBuf> = HashMap::new();
    if let Some(dir) = &dir {
        index_dir(Path::new(dir), Path::new(dir), &mut files);
        println!("{} audio files under {dir}", files.len());
    }

    // (key, SR2 resource path) worklist: every Sounds-block entry plus
    // the map ambience the EST_SOUND spawners play by name
    // (Sound.MapWater etc. — resolved by the host in the original,
    // absent from the Sounds block).
    let mut wanted: Vec<(String, String)> = defs
        .iter()
        .filter(|(_, d)| !d.path.is_empty())
        .map(|(k, d)| (k.clone(), d.path.clone()))
        .collect();
    wanted.sort();
    // Ambient names go last so paths shared with Sounds-block keys
    // resolve through the key probe first.
    for name in ambient_spawner_names() {
        wanted.push((name.clone(), name));
    }

    let mut bundle = AssetBundle::new();
    let mut packed: HashMap<String, String> = HashMap::new(); // path → source file
    let mut missing: Vec<String> = Vec::new();
    for (key, path) in wanted {
        if packed.contains_key(&path) {
            continue;
        }
        if !pkgs.is_empty() {
            let file = matrixgame_rs::platform::sound_pkg_map::pkg_file_for(&path);
            match file.and_then(|f| pkgs.iter().find_map(|pkg| pkg.read_file(f).ok())) {
                Some(bytes) => {
                    bundle.add(&path, bytes);
                    packed.insert(path.clone(), file.unwrap().to_string());
                }
                None => missing.push(format!("{key} ({path})")),
            }
            continue;
        }
        let candidates = [
            key.to_lowercase(),
            path.to_lowercase(),
            path.to_lowercase().replace('.', "/"),
        ];
        let hit = candidates.iter().find_map(|c| files.get(c));
        match hit {
            Some(f) => {
                let bytes = std::fs::read(f).expect("read audio file");
                bundle.add(&path, bytes);
                packed.insert(path.clone(), f.display().to_string());
            }
            None => missing.push(format!("{key} ({path})")),
        }
    }

    missing.sort();
    for m in &missing {
        println!("  missing: {m}");
    }
    println!(
        "packed {} samples, {} sound keys unresolved (they stay silent)",
        packed.len(),
        missing.len()
    );
    let bytes = bundle.to_zstd_bytes(19);
    std::fs::write(&out, &bytes).expect("write bundle");
    println!("wrote {out} ({} KiB)", bytes.len() / 1024);
}

/// Distinct sound names authored on EST_SOUND effect spawners across
/// all maps in robots.pkg (`3,min,max,vol0,vol1,pan0,pan1,attn,name`).
fn ambient_spawner_names() -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    let Ok(data) = std::fs::read("Data/robots.pkg") else {
        return Vec::new();
    };
    let Ok(pkg) = PkgArchive::from_bytes(data) else {
        return Vec::new();
    };
    for f in pkg.list_files() {
        if !f.ends_with(".CMAP") {
            continue;
        }
        let Ok(cmap) = pkg.read_file(&f) else { continue };
        let Ok(map) = matrixgame_rs::matrix_game::map::GameMap::from_cmap_bytes(&cmap) else {
            continue;
        };
        for (_, spec) in &map.effect_spawners {
            let parts: Vec<&str> = spec.split(',').map(|s| s.trim()).collect();
            if parts.first() == Some(&"3") {
                if let Some(name) = parts.get(8).filter(|n| !n.is_empty()) {
                    out.insert(name.to_string());
                }
            }
        }
    }
    println!("{} ambient spawner sounds across maps: {:?}", out.len(), out);
    out.into_iter().collect()
}

/// Recursive scan; keys are lowercased extension-less paths relative
/// to the root (`sound/weapplasma`), plus the bare stem (`weapplasma`)
/// so flat probes hit files in subfolders too.
fn index_dir(root: &Path, dir: &Path, out: &mut HashMap<String, std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            index_dir(root, &p, out);
            continue;
        }
        let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !EXTS.contains(&ext.to_lowercase().as_str()) {
            continue;
        }
        if let Ok(rel) = p.strip_prefix(root) {
            let rel_stem = rel
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            out.entry(rel_stem.clone()).or_insert_with(|| p.clone());
            if let Some(stem) = Path::new(&rel_stem).file_name().and_then(|s| s.to_str()) {
                out.entry(stem.to_string()).or_insert_with(|| p.clone());
            }
        }
    }
}
