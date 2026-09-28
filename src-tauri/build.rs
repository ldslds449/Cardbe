fn main() {
    println!("cargo:rerun-if-env-changed=CARDBE_BUILD_COMMIT");
    if let Ok(commit) = std::env::var("CARDBE_BUILD_COMMIT") {
        println!("cargo:rustc-env=CARDBE_BUILD_COMMIT={commit}");
    }
    tauri_build::build()
}
