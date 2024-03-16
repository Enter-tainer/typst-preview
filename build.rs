use anyhow::Result;
use vergen::EmitBuilder;

fn main() -> Result<()> {
    // Emit the instructions
    EmitBuilder::builder()
        .all_cargo()
        .build_timestamp()
        .git_sha(false)
        .git_describe(true, true, None)
        .all_rustc()
        .emit()?;

    // touch if not exists
    const ERROR_404: &str = r#"<html lang="en"><head><meta charset="UTF-8" /><title>404 Not Found</title></head><body><h1>404 Not Found</h1><p>This typst-preview is built without frontend.</p></body></html>"#;
    if !std::path::Path::new("addons/vscode/out/frontend/index.html").exists() {
        std::fs::create_dir_all("addons/vscode/out/frontend")?;
        std::fs::write("addons/vscode/out/frontend/index.html", ERROR_404)?;
    }
    Ok(())
}
