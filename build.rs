const PACKAGES: &[&str] = &["color", "direct_32", "perturbed_32"];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    for package in PACKAGES {
        wesl::Wesl::new("src/shaders")
            .build_artifact(&format!("package::{package}").parse().unwrap(), package);
    }
}
