pub fn success(msg: &str) {
    eprintln!("{} {}", console::style("✓").green().bold(), msg);
}

pub fn info(msg: &str) {
    eprintln!("{} {}", console::style("·").cyan(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", console::style("✗").red().bold(), msg);
}
