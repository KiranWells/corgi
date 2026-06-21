use std::path::Path;

const TEST_OUTPUT_DIR: &str = "target/test";
static INIT: std::sync::Once = std::sync::Once::new();

pub fn test_output_dir() -> &'static Path {
    INIT.call_once(|| {
        let _ = std::fs::remove_dir_all(Path::new(TEST_OUTPUT_DIR));
        std::fs::create_dir(Path::new(TEST_OUTPUT_DIR)).unwrap();
    });
    Path::new(TEST_OUTPUT_DIR)
}
