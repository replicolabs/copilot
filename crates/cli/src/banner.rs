use std::io::IsTerminal;

const BANNER: &str = r#"
   ___   ___   ___  ___ _      ___ _____
  / __| / _ \ | _ \|_ _| |    / _ \_   _|
 | (__ | (_) ||  _/ | || |__ | (_) || |
  \___| \___/ |_|  |___|____| \___/ |_|
"#;

const TAGLINE: &str = "Smart transaction infrastructure for Solana";

pub fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

pub fn paint(text: &str, code: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

pub fn dim(text: &str) -> String {
    paint(text, "2")
}

pub fn accent(text: &str) -> String {
    paint(text, "1;36")
}

pub fn good(text: &str) -> String {
    paint(text, "32")
}

pub fn warn(text: &str) -> String {
    paint(text, "33")
}

pub fn bad(text: &str) -> String {
    paint(text, "31")
}

pub fn print_banner() {
    println!("{}", accent(BANNER));
    println!("  {}\n", dim(TAGLINE));
}
