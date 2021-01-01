use std::fs;
use structopt::clap::Shell;

include!("src/app.rs");

const BIN_NAME: &str = "mosaic";

fn main() {
    let mut clap_app = Opt::clap();
    println!("cargo:rerun-if-changed=src/app.rs");
    let out_dir = std::env::var_os("SHELL_COMPLETION_DIR").or(std::env::var_os("OUT_DIR"));
    let out_dir = match out_dir {
        None => return,
        Some(out_dir) => out_dir,
    };

    println!("{:?}", out_dir);
    fs::create_dir_all(&out_dir).unwrap();
    clap_app.gen_completions(BIN_NAME, Shell::Bash, &out_dir);
    clap_app.gen_completions(BIN_NAME, Shell::Zsh, &out_dir);
    clap_app.gen_completions(BIN_NAME, Shell::Fish, &out_dir);
}
