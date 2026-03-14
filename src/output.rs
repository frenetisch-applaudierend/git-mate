pub fn emit_cd(path: &std::path::Path) {
    if crate::git::called_from_wrapper() {
        println!("_MATE_CD:{}", path.display());
    }
}
