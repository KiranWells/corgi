use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

const PACKAGES: &[&str] = &["color", "direct_32", "perturbed_32", "preview"];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    gen_shared_wesl();
    for package in PACKAGES {
        let mut router = wesl::Router::new();
        router.mount_fallback_resolver(wesl::FileResolver::new("src/shaders"));
        router.mount_resolver(
            "shared".parse().unwrap(),
            wesl::FileResolver::new(format!(
                "{}{}",
                std::env::var("OUT_DIR").unwrap(),
                "/shared"
            )),
        );
        let compiler = wesl::Wesl::new("").set_custom_resolver(router);
        compiler.build_artifact(&format!("package::{package}").parse().unwrap(), package);
    }
}

fn gen_shared_wesl() {
    let shared_path = PathBuf::from("src/shared").canonicalize().unwrap();
    let target_path = PathBuf::from(format!(
        "{}{}",
        std::env::var("OUT_DIR").unwrap(),
        "/shared"
    ));
    let files = find_files(&shared_path);
    for file in files {
        println!("cargo::rerun-if-changed={}", file.to_string_lossy());
        let mut text = std::fs::read_to_string(&file).unwrap();
        let mut target_file = target_path.clone();
        target_file.push(
            file.strip_prefix(&shared_path)
                .unwrap()
                .with_extension("wesl"),
        );

        // perform the necessary text substitutions to make the rs valid wesl
        let substitutions = [
            // remove wgsl compatability import
            (r"use crate::shared::wgsl_primitives::\*;", ""),
            // remove explicitly host-only lines
            (r"(?m)^.*\#\[host\]\n.*\n", ""),
            (r"(?m)^.*(\w+)!\([^)]+\);\n", ""),
            // remove attributes and derives
            (r"(?m)^ *#\[[^\]]+\]$", ""),
            // remove impl blocks
            (r"(?m)^impl[^}]+\{(.|\s)*?^\}", ""),
            // remove visibility descriptors
            (r"(?m)^( *)pub ", "$1"),
            // remove slice function parameters (these are global in wgsl)
            (r"\w+: &\[(?<type>\w+)\],", ""),
            // use -> import
            (r"(?m)^use ", "import "),
            // let mut -> var
            (r"(?m)^(\s+)let mut ", "${1}var "),
            // &mut T -> ptr<function, T>
            (r": &mut (?<i>\w+)", ": ptr<function, $i>"),
            // array type syntax
            (r"\[(?<type>\w+); (?<expr>[^\]]+)\]", "array<$type, $expr>"),
            // &mut function arguments
            (r"&mut (?<i>\w+)", "&$i"),
            // usize -> u32
            (r"\busize\b", "u32"),
            // 1_usize -> 1u
            (r"([0-9a-fA-F]+)_?usize", "${1}u"),
            // 1_u32 -> 1u
            (r"([0-9a-fA-F]+)_?u32", "${1}u"),
            (r"([0-9a-fA-F]+)_?i32", "${1}"),
            // 2.0_f32 -> 2.0f
            (r"(\d+)_?f32", "${1}f"),
            // Vec3::new -> Vec3
            (r"::(new|splat)\(", "("),
            // swizzles: v.rgb() -> v.rgb
            (r"\.([rgbaxyzw]+)\(\)", ".$1"),
            // Vec3f -> vec3f
            (r"Vec(?<suffix>\d(f|u)?)", "vec$suffix"),
            (r"Mat(?<suffix>\dx\d(f|u)?)", "mat$suffix"),
            // function name differences
            (r"\bpowf\b", "pow"),
            (r"\bln\(", "log("),
            (r"\bbitcast\(", "bitcast<u32>("),
            // for loop syntax
            (
                r"for (?<var>\w+) in (?<start>[^\.]+)\.\.(?<end>\S+) \{",
                "for (var $var = $start; $var < $end; $var = $var + 1) {",
            ),
            // member functions -> global functions: 2.0.cos() -> cos(2.0)
            // These (and the casts) have issues parsing nested parentheses, as
            // this is a limitation of regex.
            (
                r"(?m)(?:\((?<value2>(?:[^()]|\([^()]*\))+)\)|(?<value1>(?:\w|\.|\([^()]*\))+))\s*\.(?<func>\w+)\((?<args>([^()]|\([^()]*\))+)\)",
                "$func($value1$value2, $args)",
            ),
            (
                r"(?m)(?:\((?<value2>(?:[^()]|\([^()]*\))+)\)|(?<value1>(?:\w|\.|\([^()]*\))+))\s*\.(?<func>\w+)\(\)",
                "$func($value1$value2)",
            ),
            // casts: x as u32 -> u32(x)
            (
                r"(?:\((?<value2>(?:[^()]|\([^()]*\))+)\)|(?<value1>(?:\w|\.|\([^()]*\))+)) as (?<type>(f|u|i)32)",
                "$type($value1$value2)",
            ),
            // Struct constructor; this is brittle and only supports struct
            // constructions that use the shortcut syntax
            (r"(?m)= (?<n>[A-Z]\w+) \{(?<i>[^}]+)\}", "= $n($i)"),
            // match blocks; these are limited to unsigned constants
            (
                r"(?m)match (?<input>\S+) \{(?<block>[^}]+)\}",
                "switch $input {$block}",
            ),
            (
                r"(?m)^ *\w+ if \w+ == *(?<input>[^=]+) => \{(?<block>[^}]+)\}",
                "case $input {$block}",
            ),
            (
                r"(?m)^ *(?<input>\d+_?(u32|usize)?) => \{(?<block>[^}]+)\}",
                "case $input {$block}",
            ),
            (r"(?m)^ *_ => \{(?<block>[^}]*)\}", "default {$block}"),
        ];

        for (pattern, replacement) in substitutions {
            let re = regex::Regex::new(pattern).unwrap();
            loop {
                let new_text = re.replace_all(&text, replacement);
                if text == new_text {
                    break;
                }
                text = new_text.to_string();
            }
        }
        if !target_file.parent().unwrap().exists() {
            std::fs::create_dir_all(target_file.parent().unwrap()).unwrap();
        }
        let mut output = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(target_file)
            .unwrap();
        print!("{text}");
        output.write_all(text.as_bytes()).unwrap();
    }
}

fn find_files(path: &PathBuf) -> Vec<PathBuf> {
    let mut files = vec![];
    let dir = std::fs::read_dir(path).unwrap();
    for file in dir.flatten() {
        if file.path().extension() == Some(OsStr::from_bytes(b"rs"))
            && !file.path().ends_with("mod.rs")
        {
            files.push(file.path().canonicalize().unwrap());
        }
        if file.path().is_dir() {
            files.append(&mut find_files(&file.path()));
        }
    }
    files
}
