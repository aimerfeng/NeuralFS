//! Build script for NeuralFS
//! 
//! Handles:
//! - Copying DLLs from deps/ to target directory
//! - Tauri build configuration
//! - ONNX Runtime path configuration

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Run Tauri build
    tauri_build::build();

    // Copy DLLs on Windows
    #[cfg(target_os = "windows")]
    copy_windows_dlls();

    // Set ONNX Runtime library path
    configure_onnx_runtime();

    // Print rerun conditions
    println!("cargo:rerun-if-changed=deps/");
    println!("cargo:rerun-if-changed=build.rs");
}

/// Copy DLLs from deps/ directory to target output
#[cfg(target_os = "windows")]
fn copy_windows_dlls() {
    let deps_dir = Path::new("deps");
    
    if !deps_dir.exists() {
        println!("cargo:warning=deps/ directory not found, skipping DLL copy");
        return;
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("Could not find target directory");

    // Copy all DLLs from deps/
    if let Ok(entries) = fs::read_dir(deps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "dll").unwrap_or(false) {
                let filename = path.file_name().unwrap();
                let dest = target_dir.join(filename);
                
                if let Err(e) = fs::copy(&path, &dest) {
                    println!("cargo:warning=Failed to copy {:?}: {}", path, e);
                } else {
                    println!("cargo:warning=Copied {:?} to {:?}", path, dest);
                }
            }
        }
    }
}

/// Configure ONNX Runtime library path
fn configure_onnx_runtime() {
    // Check for ONNX Runtime in common locations
    let onnx_paths = [
        "deps/onnxruntime",
        "C:/Program Files/onnxruntime",
        "/usr/local/lib",
        "/opt/onnxruntime/lib",
    ];

    for path in &onnx_paths {
        if Path::new(path).exists() {
            println!("cargo:rustc-link-search=native={}", path);
            println!("cargo:warning=Found ONNX Runtime at: {}", path);
            return;
        }
    }

    // Check environment variable
    if let Ok(onnx_path) = env::var("ORT_LIB_LOCATION") {
        println!("cargo:rustc-link-search=native={}", onnx_path);
        println!("cargo:warning=Using ONNX Runtime from ORT_LIB_LOCATION: {}", onnx_path);
        return;
    }

    println!("cargo:warning=ONNX Runtime not found. Set ORT_LIB_LOCATION or place in deps/onnxruntime");
}
