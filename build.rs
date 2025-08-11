fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    wesl::Wesl::new("src/shaders").build_artifact(&"package::color".parse().unwrap(), "color");
    wesl::Wesl::new("src/shaders")
        .build_artifact(&"package::calculate".parse().unwrap(), "calculate");
}
