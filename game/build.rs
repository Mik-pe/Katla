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
            let source = find_vulkan_dylib();

            if let Some(dylib) = source {
                let full_path = &dylib.canonicalize().unwrap_or_else(|_| dylib.clone());

                match std::os::unix::fs::symlink(full_path, &libvulkan) {
                    Ok(()) => {
                        println!(
                            "cargo:warning=Linked libvulkan.dylib -> {}",
                            dylib.display()
                        );
                    }
                    Err(e) => {
                        println!("cargo:warning=Failed to symlink libvulkan.dylib: {}", e);
                        print_install_hint();
                    }
                }
            } else {
                println!("cargo:warning=Vulkan runtime not found on macOS.");
                print_install_hint();
            }
        }

        println!("cargo:rerun-if-env-changed=DYLD_LIBRARY_PATH");
        println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    }
}

/// Search for a Vulkan dylib on macOS in priority order:
/// 1. `$VULKAN_SDK/lib/libvulkan.dylib` (LunarG SDK, env var set)
/// 2. `~/VulkanSDK/*/macOS/lib/libvulkan.dylib` (auto-discover SDK)
/// 3. `$VULKAN_SDK/lib/libMoltenVK.dylib` (LunarG SDK fallback)
/// 4. `~/VulkanSDK/*/macOS/lib/libMoltenVK.dylib` (auto-discover SDK fallback)
/// 5. Homebrew MoltenVK
fn find_vulkan_dylib() -> Option<std::path::PathBuf> {
    // 1. LunarG SDK via VULKAN_SDK env var
    if let Ok(sdk) = std::env::var("VULKAN_SDK") {
        let sdk_lib = std::path::Path::new(&sdk).join("lib");
        let libvulkan = sdk_lib.join("libvulkan.dylib");
        if libvulkan.exists() {
            return Some(libvulkan);
        }
        let moltenvk = sdk_lib.join("libMoltenVK.dylib");
        if moltenvk.exists() {
            return Some(moltenvk);
        }
    }

    // 2. Auto-discover LunarG SDK in ~/VulkanSDK/
    if let Ok(home) = std::env::var("HOME") {
        let sdk_base = std::path::Path::new(&home).join("VulkanSDK");
        if let Ok(entries) = std::fs::read_dir(&sdk_base) {
            let mut versions: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .collect();
            versions.sort_by_key(|e| e.file_name());

            for entry in versions.into_iter().rev() {
                let lib_dir = entry.path().join("macOS").join("lib");
                let libvulkan = lib_dir.join("libvulkan.dylib");
                if libvulkan.exists() {
                    return Some(libvulkan);
                }
                let moltenvk = lib_dir.join("libMoltenVK.dylib");
                if moltenvk.exists() {
                    return Some(moltenvk);
                }
            }
        }
    }

    // 3. Homebrew MoltenVK
    for candidate in [
        "/opt/homebrew/lib/libMoltenVK.dylib",
        "/usr/local/lib/libMoltenVK.dylib",
    ] {
        let path = std::path::Path::new(candidate);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    None
}

fn print_install_hint() {
    println!("cargo:warning=Install the LunarG Vulkan SDK from https://vulkan.lunarg.com/sdk/home");
    println!("cargo:warning=Or install MoltenVK via Homebrew: brew install molten-vk");
}
