//! Oumu-Setup.exe: installs Oumu for this user (no admin rights).
#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = install::Opts::from_args(&args);
    let payload = include_bytes!(env!("OUMU_PAYLOAD"));
    match install::install(&install::OUMU, payload, &o, |_| Ok(())) {
        Ok(_) => {
            if o.silent {
                println!("installed");
            }
        }
        Err(e) => {
            if o.silent {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            install::tell("Oumu setup", &format!("Setup didn't finish:\n\n{e:#}"), true);
        }
    }
}
