use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let (major, minor, patch, build) = parse_version_quad(&version);
    let file_version = format!("{major}.{minor}.{patch}.{build}");
    let file_version_commas = format!("{major},{minor},{patch},{build}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let rc_path = out_dir.join("version.rc");
    let rc = format!(
        r#"#include <winres.h>

VS_VERSION_INFO VERSIONINFO
 FILEVERSION {file_version_commas}
 PRODUCTVERSION {file_version_commas}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "SVG converter (PNG / ICO / ICNS / TTF / WOFF / WOFF2)"
            VALUE "FileVersion", "{file_version}"
            VALUE "InternalName", "svgc.exe"
            VALUE "LegalCopyright", "Copyright (C) emako"
            VALUE "OriginalFilename", "svgc.exe"
            VALUE "ProductName", "svgc"
            VALUE "ProductVersion", "{file_version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    );
    fs::write(&rc_path, rc).expect("write version.rc");

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("compile version.rc");
}

fn parse_version_quad(version: &str) -> (u16, u16, u16, u16) {
    let mut parts = version.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
