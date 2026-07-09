pub mod color;
mod commands;
#[cfg(test)]
mod test_support;

use commands::{analyze, build, bundle_id, dev, devices, doctor, new, snapshot};
use commands::package;
use commands::run;
use commands::workspace;

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
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("run") => {
            match run::RunOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = run::run(opts) {
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("build") => {
            match build::BuildOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = build::run(opts) {
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
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
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("new") => {
            match new::NewOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = new::run(opts) {
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("bundle-id") => {
            match bundle_id::BundleIdOptions::from_args(rest) {
                Ok(opts) => {
                    if let Err(e) = bundle_id::run(opts) {
                        eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("doctor") => {
            match doctor::DoctorOptions::from_args(rest) {
                Ok(_) => doctor::run(),
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("devices") => {
            match devices::DevicesOptions::from_args(rest) {
                Ok(_) => devices::run(),
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("check") => {
            let verbose = rest.iter().any(|a| a == "--verbose" || a == "-v");
            let result = workspace::run_check(verbose);
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
            println!("{}", result.summary());
            std::process::exit(result.exit_code);
        }
        Some("test") => {
            let filter = rest.first().map(String::as_str);
            let result = workspace::run_test(filter);
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
            println!("{}", result.summary());
            std::process::exit(result.exit_code);
        }
        Some("lint") => {
            let result = workspace::run_lint();
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
            println!("{}", result.summary());
            std::process::exit(result.exit_code);
        }
        Some("fmt") => {
            let result = workspace::run_fmt_check();
            print!("{}", result.stdout);
            eprint!("{}", result.stderr);
            println!("{}", result.summary());
            std::process::exit(result.exit_code);
        }
        Some("analyze") => {
            match analyze::AnalyzeOptions::from_args(rest) {
                Ok(opts) => {
                    match analyze::run_analyze(&opts) {
                        Ok(report) => {
                            println!("{}", report.summary());
                            if opts.verbose {
                                println!("  Members: {}", report.member_list());
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
                    std::process::exit(1);
                }
            }
        }
        Some("snapshot") => {
            match snapshot::SnapshotOptions::from_args(rest) {
                Ok(opts) => {
                    let result = snapshot::run_snapshot(&opts);
                    print!("{}", result.stdout);
                    eprint!("{}", result.stderr);
                    println!("{}", result.summary());
                    std::process::exit(result.exit_code);
                }
                Err(e) => {
                    eprintln!("{}", crate::color::red(&format!("error: {}", e)));
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
    println!("  new <name>          Scaffold a new TEZZERA app (interactive platform picker)
    --platforms <list> macos,windows,linux,web,ios,android (skips the prompt); --all for every platform
    --bundle-id <id>   app bundle/package id (skips that prompt too)");
    println!("  run                 Build + run the app on a platform
    --mac/--win/--lnx  shorthand for --target macos|windows|linux; --target web/ios also work
    --port <n> (web), --device <name> (ios)");
    println!("  bundle-id [<id>]    Print (no arg) or update the app's bundle id everywhere it's embedded");
    println!("  doctor            Check this machine's toolchains for every target (Flutter-doctor-style)");
    println!("  devices           List available run targets across platforms (id works with `run --device`)");
    println!("  dev               Start desktop app in dev mode (cargo run)");
    println!("  build             Build the app for a target platform");
    println!("  package           Bundle for distribution (.app / .deb / .exe)");
    println!("  check             Run `cargo check --workspace`");
    println!("  test [filter]     Run `cargo test --workspace` (optional test filter)");
    println!("  lint              Run `cargo clippy --workspace -- -D warnings`");
    println!("  fmt               Run `cargo fmt --workspace --check`");
    println!("  analyze           Workspace health: crate count, member list");
    println!("  snapshot          Run an example binary and save its PNG output");
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
