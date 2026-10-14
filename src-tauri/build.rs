//! Tauri build script.
//!
//! Besides the usual code generation it makes sure standalone test binaries get
//! the same common-controls v6 manifest as the application. Without it the
//! Windows loader resolves `comctl32.dll` v5, which does not export
//! `TaskDialogIndirect` (imported through tauri) and every integration test
//! would die with `STATUS_ENTRYPOINT_NOT_FOUND` before `main` runs.

use std::path::PathBuf;

const TEST_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let manifest = out_dir.join("test-manifest.xml");
        if std::fs::write(&manifest, TEST_MANIFEST).is_ok() {
            println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
                manifest.display()
            );
        }
    }

    tauri_build::build()
}
