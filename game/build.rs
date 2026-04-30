fn main() {
    if cfg!(target_os = "macos") {
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let deps_dir = out_dir
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("failed to resolve target dir from OUT_DIR");

        let libvulkan = deps_dir.join("libvulkan.dylib");

        if !libvulkan.exists() {
            let candidates = [
                "/opt/homebrew/lib/libMoltenVK.dylib",
                "/usr/local/lib/libMoltenVK.dylib",
            ];

            let moltenvk = candidates.iter().find(|p| std::path::Path::new(p).exists());

            if let Some(moltenvk) = moltenvk {
                let full_path = std::path::Path::new(moltenvk)
                    .canonicalize()
                    .unwrap_or_else(|_| std::path::PathBuf::from(moltenvk));

                match std::os::unix::fs::symlink(&full_path, &libvulkan) {
                    Ok(()) => {
                        println!("cargo:warning=Linked libvulkan.dylib -> {}", moltenvk);
                    }
                    Err(e) => {
                        println!("cargo:warning=Failed to symlink libvulkan.dylib: {}", e);
                        println!("cargo:warning=Install MoltenVK: brew install molten-vk");
                    }
                }
            } else {
                println!(
                    "cargo:warning=MoltenVK not found. Install it with: brew install molten-vk"
                );
            }
        }

        println!("cargo:rerun-if-env-changed=DYLD_LIBRARY_PATH");
    }
}
