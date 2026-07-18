use std::{env, fs, path::PathBuf};

const OAUTH_BUILD_KEYS: [&str; 4] = [
    "TRUEMAIL_YANDEX_CLIENT_ID",
    "TRUEMAIL_YANDEX_REDIRECT_URI",
    "TRUEMAIL_GOOGLE_CLIENT_ID",
    "TRUEMAIL_GOOGLE_CLIENT_SECRET",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dotenv = manifest_dir.join("..").join("..").join(".env");

    println!("cargo:rerun-if-changed={}", dotenv.display());
    for key in OAUTH_BUILD_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let contents = fs::read_to_string(&dotenv).unwrap_or_default();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !OAUTH_BUILD_KEYS.contains(&name) || env::var_os(name).is_some() {
            continue;
        }

        let value = value.trim().trim_matches(['"', '\'']);
        if !value.is_empty() {
            println!("cargo:rustc-env={name}={value}");
        }
    }
}
