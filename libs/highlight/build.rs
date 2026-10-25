use std::{env, path::PathBuf};

use syntect::dumps::dump_to_uncompressed_file;

fn main() {
    let extra = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("syntaxes");
    println!("cargo:rerun-if-changed={}", extra.display());

    let mut builder = two_face::syntax::extra_newlines().into_builder();
    if extra.is_dir() {
        builder
            .add_from_folder(&extra, true)
            .expect("extra syntaxes should load");
    }
    let syntax_set = builder.build();

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("syntaxes.bin");
    dump_to_uncompressed_file(&syntax_set, out).expect("syntax set should dump");
}
