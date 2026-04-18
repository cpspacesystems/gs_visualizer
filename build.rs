use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let schema_dir = manifest_dir.join("../flatbuffers/foxglove");
    let generated_rust_dir = manifest_dir.join("schemas/rust");
    let generated_bfbs_dir = manifest_dir.join("schemas/bfbs");

    let schema_files = collect_schema_files(&schema_dir).unwrap_or_else(|err| {
        panic!(
            "failed to read Foxglove schemas from {}: {err}",
            schema_dir.display()
        )
    });

    if schema_files.is_empty() {
        panic!(
            "no .fbs files found in {}; populate the sibling flatbuffers repo first",
            schema_dir.display()
        );
    }

    for schema_file in &schema_files {
        println!("cargo:rerun-if-changed={}", schema_file.display());
    }

    recreate_dir(&generated_rust_dir)
        .unwrap_or_else(|err| panic!("failed to prepare {}: {err}", generated_rust_dir.display()));
    recreate_dir(&generated_bfbs_dir)
        .unwrap_or_else(|err| panic!("failed to prepare {}: {err}", generated_bfbs_dir.display()));

    run_flatc(&schema_dir, &generated_rust_dir, &schema_files, &["--rust"]);
    run_flatc(
        &schema_dir,
        &generated_bfbs_dir,
        &schema_files,
        &["--binary", "--schema"],
    );
}

fn collect_schema_files(schema_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(schema_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("fbs") {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

fn recreate_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    fs::create_dir_all(path)
}

fn run_flatc(schema_dir: &Path, output_dir: &Path, schema_files: &[PathBuf], args: &[&str]) {
    let mut command = Command::new("flatc");
    command.arg("-I").arg(schema_dir).arg("-o").arg(output_dir);

    for arg in args {
        command.arg(arg);
    }

    for schema_file in schema_files {
        command.arg(schema_file);
    }

    let output = command.output().unwrap_or_else(|err| {
        panic!("failed to run `flatc`: {err}. Install the FlatBuffers compiler to build this crate")
    });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("flatc failed while generating schemas:\n{stderr}");
    }
}
