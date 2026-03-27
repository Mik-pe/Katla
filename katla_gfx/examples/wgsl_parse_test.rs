/// Minimal naga 28 WGSL parser test.
/// Usage: cargo run -p katla_gfx --example wgsl_parse_test -- <path-to-shader.wgsl>
/// Parses the WGSL file and reports the exact error location with line numbers.

use naga::front::wgsl;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: wgsl_parse_test <shader.wgsl>");
        return ExitCode::from(1);
    }

    let path = &args[1];
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            return ExitCode::from(1);
        }
    };

    println!("Parsing {} ({} bytes, {} lines)...", path, src.len(), src.lines().count());

    match wgsl::parse_str(&src) {
        Ok(_) => {
            println!("OK: Shader parsed successfully.");
            ExitCode::SUCCESS
        }
        Err(err) => {
            let msg = err.message();
            println!("FAILED: {}", msg);

            // Use the built-in pretty-printer which includes line/col info
            let pretty = err.emit_to_string_with_path(&src, path);
            println!("{}", pretty);

            // Also manually extract location from labels
            for (span, label) in err.labels() {
                if let Some(range) = span.to_range() {
                    let excerpt = &src[range.clone()];
                    let loc = span.location(&src);
                    println!(
                        "  Label: span bytes {}..{} = line {} col {}, text: {:?}",
                        range.start,
                        range.end,
                        loc.line_number,
                        loc.line_position,
                        excerpt
                    );
                }
            }

            ExitCode::from(1)
        }
    }
}
