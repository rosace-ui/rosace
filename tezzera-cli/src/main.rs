mod commands;

use commands::{build, dev, new};
use commands::package;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // args[0] is the binary name, args[1] is the subcommand
    let subcommand = args.get(1).map(String::as_str);
    let rest: &[String] = if args.len() > 2 { &args[2..] } else { &[] };

    match subcommand {
        Some("dev") => {
            match dev::DevOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = dev::run(opts) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some("build") => {
            match build::BuildOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = build::run(opts) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    eprintln!();
                    print_usage();
                    std::process::exit(1);
                }
            }
        }
        Some("package") => {
            match package::PackageOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = package::run(opts) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some("new") => {
            match new::NewOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = new::run(opts) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            print_usage();
        }
        Some(unknown) => {
            eprintln!("error: unknown command '{}'", unknown);
            eprintln!();
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("tzr — TEZZERA CLI");
    println!();
    println!("USAGE:");
    println!("  tzr <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  new <name>          Scaffold a new TEZZERA app project
    --template <name> Template: counter (default), nav-app, form-app, dashboard");
    println!("  dev               Start desktop app in dev mode (cargo run)");
    println!("  build             Build the app for a target platform");
    println!("  package           Bundle for distribution (.app / .deb / .exe)");
    println!("  help              Print this message");
    println!();
    println!("OPTIONS (dev):");
    println!("  --target web      Build WASM and serve at http://localhost:3000");
    println!("  --port <n>        Port for web dev server (default: 3000)");
    println!("  --watch           Rebuild on source changes");
    println!();
    println!("OPTIONS (build):");
    println!("  --target <target> Build target: desktop | web");
    println!();
    println!("OPTIONS (package):");
    println!("  --name <name>     App name (default: from Cargo.toml)");
    println!("  --version <ver>   App version (default: from Cargo.toml)");
    println!("  --out <dir>       Output directory (default: dist/)");
    println!();
    println!("EXAMPLES:");
    println!("  tzr dev");
    println!("  tzr dev --target web");
    println!("  tzr dev --target web --port 8080");
    println!("  tzr build --target desktop");
    println!("  tzr build --target web");
}
