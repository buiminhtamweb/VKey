use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is missing")?);
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or("failed to resolve workspace root")?;
    let icon_source = find_icon_source(workspace_root)?
        .ok_or("could not find a vkey_icon_*.png or vkey_logo_*.png asset in the workspace root")?;

    println!("cargo:rerun-if-changed={}", icon_source.display());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    let icon_png = out_dir.join("vkey-app-icon.png");
    fs::copy(&icon_source, &icon_png)?;
    println!(
        "cargo:rustc-env=VKEY_APP_ICON_PNG_PATH={}",
        icon_png.display()
    );

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon_ico = out_dir.join("vkey-app-icon.ico");
        write_windows_icon(&icon_source, &icon_ico)?;
        winresource::WindowsResource::new()
            .set_icon(icon_ico.to_string_lossy().as_ref())
            .compile()?;
    }

    Ok(())
}

fn find_icon_source(workspace_root: &Path) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    for prefix in ["vkey_icon_", "vkey_logo_"] {
        let mut matches = fs::read_dir(workspace_root)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| matches_icon_name(path, prefix))
            .collect::<Vec<_>>();
        matches.sort();

        if let Some(path) = matches.into_iter().next() {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn matches_icon_name(path: &Path, prefix: &str) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };

    name.starts_with(prefix) && name.ends_with(".png")
}

fn write_windows_icon(
    source_png: &Path,
    target_ico: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = image::ImageReader::open(source_png)?
        .with_guessed_format()?
        .decode()?
        .into_rgba8();
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);

    for size in [16, 24, 32, 40, 48, 64, 128, 256] {
        let resized =
            image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
        let icon_image = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        icon_dir.add_entry(ico::IconDirEntry::encode(&icon_image)?);
    }

    let mut file = File::create(target_ico)?;
    icon_dir.write(&mut file)?;
    Ok(())
}
