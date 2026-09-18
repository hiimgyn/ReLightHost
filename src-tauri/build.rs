use std::fs;
use std::path::Path;

fn main() {
  // Tauri validates at build-script execution time that every file declared under
  // `bundle.resources` in `tauri.conf.json` already exists on disk.
  // Because `vst3_sandbox_host.exe` is built as a binary target (`src/bin/`) of this
  // same crate, on clean checkouts / CI runners it does not exist when build.rs first runs.
  // Emitting a placeholder file lets `tauri_build::build()` proceed with its configuration checks.
  // The actual binary compilation will overwrite this file before Tauri bundles the installer.
  let release_bin = Path::new("target/release/vst3_sandbox_host.exe");
  if !release_bin.exists() {
    if let Some(parent) = release_bin.parent() {
      let _ = fs::create_dir_all(parent);
    }
    let _ = fs::File::create(release_bin);
  }

  tauri_build::build()
}

