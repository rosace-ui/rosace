mod commands;

use commands::{build, dev};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // args[0] is the binary name, args[1] is the subcommand
    let subcommand = args.get(1).map(String::as_str);
    let rest: &[String] = if args.len() > 2 { &args[2..] } else { &[] };

    match subcommand {
        Some("dev") => {
            let opts = dev::DevOptions::from_args(rest);
            dev::run(opts);
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
    println!("  dev               Start dev server with terminal trace output");
    println!("  build             Build the app for a target platform");
    println!("  help              Print this message");
    println!();
    println!("OPTIONS (dev):");
    println!("  --trace=<filter>  Trace filter: all | state | network | performance | <ComponentName>");
    println!();
    println!("OPTIONS (build):");
    println!("  --target <target> Build target: desktop (Phase 1)");
    println!();
    println!("EXAMPLES:");
    println!("  tzr dev --trace=state");
    println!("  tzr build --target desktop");
}
