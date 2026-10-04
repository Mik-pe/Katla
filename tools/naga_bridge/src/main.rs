//! The compiler executable exchanges bounded files and never loads into the editor.
use std::{
    env, fs,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 4 || args[0] != "--request" || args[2] != "--output" {
        return Err(
            "usage: katla-shader-compiler --request <input.json> --output <artifact.json>".into(),
        );
    }
    let mut request = Vec::new();
    fs::File::open(&args[1])?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut request)?;
    let bytes = katla_naga_compiler::compile_json(&request)?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("compiler artifact exceeds its byte limit".into());
    }
    let path = Path::new(&args[3]);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "{error}");
            ExitCode::FAILURE
        }
    }
}
