pub fn emit_cd(path: &std::path::Path) {
    if crate::git::called_from_wrapper() {
        println!("_MATE_CD:{}", path.display());
    }
}

pub fn success(msg: &str) {
    eprintln!("{} {}", console::style("✓").green().bold(), msg);
}

pub fn info(msg: &str) {
    eprintln!("{} {}", console::style("·").cyan(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", console::style("✗").red().bold(), msg);
}
