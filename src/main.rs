//! svgc — convert SVG to PNG, ICO, ICNS, or icon fonts (TTF/WOFF/WOFF2).

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use clap::Parser;
use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use icns::{IconFamily, Image as IcnsImage, PixelFormat};
use resvg::tiny_skia::{IntSize, Pixmap, Transform};
use resvg::usvg::{self, Tree};
use webfont_generator::{FontType, GenerateWebfontsOptions};

/// Convert SVG to PNG, ICO, ICNS, or icon fonts (TTF/WOFF/WOFF2).
#[derive(Debug, Parser)]
#[command(
    name = "svgc",
    version,
    about = "Convert SVG to PNG, ICO, ICNS, or icon fonts (TTF/WOFF/WOFF2)",
    arg_required_else_help = true,
    after_help = "Examples:\n  svgc icon.svg\n  svgc icon.svg icon.ico\n  svgc icon.svg -o out.icns --size 512\n  svgc icons/\n  svgc icons/ -o dist/ --name myicons\n  svgc icons/ -f woff2"
)]
struct Cli {
    /// Input SVG file, or a directory of SVG icons for font output
    input: PathBuf,

    /// Output path (file for images; directory for fonts)
    #[arg(value_name = "OUTPUT")]
    output_pos: Option<PathBuf>,

    /// Output path (overrides the positional OUTPUT)
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Output format: png|ico|icns (file) or ttf|woff|woff2 (directory; default all three)
    #[arg(short, long, value_name = "FMT")]
    format: Option<Format>,

    /// Font family / base file name (font mode only; default: directory name)
    #[arg(long, value_name = "NAME")]
    name: Option<String>,

    /// Pixel size (PNG width/height; single size for ICO/ICNS)
    #[arg(short, long, value_name = "PX")]
    size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Ico,
    Icns,
    Ttf,
    Woff,
    Woff2,
}

impl Format {
    fn as_str(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Ico => "ico",
            Format::Icns => "icns",
            Format::Ttf => "ttf",
            Format::Woff => "woff",
            Format::Woff2 => "woff2",
        }
    }

    fn extension(self) -> &'static str {
        self.as_str()
    }

    fn is_font(self) -> bool {
        matches!(self, Format::Ttf | Format::Woff | Format::Woff2)
    }

    fn is_image(self) -> bool {
        matches!(self, Format::Png | Format::Ico | Format::Icns)
    }

    fn to_font_type(self) -> Option<FontType> {
        match self {
            Format::Ttf => Some(FontType::Ttf),
            Format::Woff => Some(FontType::Woff),
            Format::Woff2 => Some(FontType::Woff2),
            _ => None,
        }
    }
}

impl FromStr for Format {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Format::Png),
            "ico" => Ok(Format::Ico),
            "icns" => Ok(Format::Icns),
            "ttf" => Ok(Format::Ttf),
            "woff" => Ok(Format::Woff),
            "woff2" => Ok(Format::Woff2),
            other => bail!("unknown format '{other}' (use png, ico, icns, ttf, woff, or woff2)"),
        }
    }
}

fn format_from_path(path: &Path) -> Option<Format> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|e| Format::from_str(e).ok())
}

fn default_output(input: &Path, format: Format) -> PathBuf {
    input.with_extension(format.extension())
}

fn output_arg(cli: &Cli) -> Option<PathBuf> {
    cli.output.clone().or_else(|| cli.output_pos.clone())
}

fn resolve_image_output_and_format(cli: &Cli) -> Result<(PathBuf, Format)> {
    let out = output_arg(cli).unwrap_or_else(|| default_output(&cli.input, Format::Png));

    let format = if let Some(f) = cli.format {
        if f.is_font() {
            bail!("format '{}' requires a directory of SVG icons as input", f.as_str());
        }
        f
    } else if let Some(f) = format_from_path(&out) {
        if f.is_font() {
            bail!("format '{}' requires a directory of SVG icons as input", f.as_str());
        }
        f
    } else if output_arg(cli).is_none() {
        Format::Png
    } else {
        bail!(
            "cannot detect format from '{}'; use -f png|ico|icns",
            out.display()
        );
    };

    let out = if out.extension().is_none() {
        out.with_extension(format.extension())
    } else {
        out
    };

    Ok((out, format))
}

fn collect_svg_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        bail!("no .svg files found in {}", dir.display());
    }
    Ok(files)
}

fn default_font_name(dir: &Path) -> String {
    dir.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .unwrap_or("iconfont")
        .to_owned()
}

fn resolve_font_types(cli: &Cli) -> Result<Vec<FontType>> {
    match cli.format {
        None => Ok(vec![FontType::Ttf, FontType::Woff, FontType::Woff2]),
        Some(f) if f.is_font() => Ok(vec![f.to_font_type().unwrap()]),
        Some(f) => bail!(
            "format '{}' is for a single SVG file; use ttf, woff, or woff2 for font mode",
            f.as_str()
        ),
    }
}

fn run_font(cli: &Cli) -> Result<()> {
    if cli.size.is_some() {
        bail!("--size is only valid for PNG/ICO/ICNS conversion");
    }
    if let Some(f) = cli.format {
        if f.is_image() {
            bail!(
                "format '{}' is for a single SVG file; pass a .svg file or use -f ttf|woff|woff2",
                f.as_str()
            );
        }
    }

    let svgs = collect_svg_files(&cli.input)?;
    let font_name = cli
        .name
        .clone()
        .unwrap_or_else(|| default_font_name(&cli.input));
    let dest = output_arg(cli).unwrap_or_else(|| PathBuf::from("."));
    if dest.exists() && dest.is_file() {
        bail!("font output path must be a directory: {}", dest.display());
    }
    fs::create_dir_all(&dest).with_context(|| format!("failed to create {}", dest.display()))?;

    let types = resolve_font_types(cli)?;
    let files: Vec<String> = svgs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let options = GenerateWebfontsOptions {
        dest: dest.to_string_lossy().into_owned(),
        files,
        font_name: Some(font_name.clone()),
        types: Some(types.clone()),
        css: Some(false),
        html: Some(false),
        ts: Some(false),
        write_files: Some(true),
        ..Default::default()
    };

    webfont_generator::generate_sync(options, None)
        .with_context(|| format!("failed to generate font from {}", cli.input.display()))?;

    let labels: Vec<&str> = types
        .iter()
        .map(|t| match t {
            FontType::Ttf => "ttf",
            FontType::Woff => "woff",
            FontType::Woff2 => "woff2",
            _ => "?",
        })
        .collect();
    println!(
        "{} -> {}/{} ({} glyphs, {})",
        cli.input.display(),
        dest.display(),
        font_name,
        svgs.len(),
        labels.join("+")
    );
    Ok(())
}

fn load_tree(path: &Path) -> Result<Tree> {
    let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    Tree::from_data(&data, &options).with_context(|| format!("failed to parse SVG {}", path.display()))
}

fn render_rgba(tree: &Tree, size: u32) -> Result<Vec<u8>> {
    if size == 0 {
        bail!("size must be greater than 0");
    }

    let mut pixmap =
        Pixmap::new(size, size).context("failed to allocate pixmap (size may be too large)")?;

    let svg_size = tree.size();
    let sx = size as f32 / svg_size.width();
    let sy = size as f32 / svg_size.height();
    let scale = sx.min(sy);
    let dx = (size as f32 - svg_size.width() * scale) / 2.0;
    let dy = (size as f32 - svg_size.height() * scale) / 2.0;
    let transform = Transform::from_row(scale, 0.0, 0.0, scale, dx, dy);

    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap.take())
}

fn png_size(tree: &Tree, override_size: Option<u32>) -> u32 {
    if let Some(s) = override_size {
        return s.max(1);
    }
    let w = tree.size().width().ceil() as u32;
    let h = tree.size().height().ceil() as u32;
    w.max(h).max(1)
}

fn write_png(path: &Path, rgba: &[u8], size: u32) -> Result<()> {
    let int_size = IntSize::from_wh(size, size).context("invalid PNG size")?;
    let pixmap = Pixmap::from_vec(rgba.to_vec(), int_size)
        .context("invalid RGBA buffer for PNG encoding")?;
    let png = pixmap.encode_png().context("PNG encode failed")?;
    fs::write(path, png).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];
const ICNS_SIZES: &[u32] = &[16, 32, 64, 128, 256, 512, 1024];

fn write_ico(path: &Path, tree: &Tree, sizes: &[u32]) -> Result<()> {
    let mut dir = IconDir::new(ResourceType::Icon);
    for &size in sizes {
        let rgba = render_rgba(tree, size)?;
        let image = IconImage::from_rgba_data(size, size, rgba);
        dir.add_entry(IconDirEntry::encode(&image).with_context(|| {
            format!("failed to encode {size}x{size} ICO entry")
        })?);
    }
    let file = fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    dir.write(BufWriter::new(file))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn write_icns(path: &Path, tree: &Tree, sizes: &[u32]) -> Result<()> {
    let mut family = IconFamily::new();
    for &size in sizes {
        let rgba = render_rgba(tree, size)?;
        let image = IcnsImage::from_data(PixelFormat::RGBA, size, size, rgba)
            .with_context(|| format!("failed to build {size}x{size} ICNS image"))?;
        family
            .add_icon(&image)
            .with_context(|| format!("failed to add {size}x{size} ICNS icon"))?;
    }
    let file = fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    family
        .write(BufWriter::new(file))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn sizes_for(format: Format, override_size: Option<u32>) -> Vec<u32> {
    if let Some(s) = override_size {
        return vec![s.max(1)];
    }
    match format {
        Format::Ico => ICO_SIZES.to_vec(),
        Format::Icns => ICNS_SIZES.to_vec(),
        _ => Vec::new(),
    }
}

fn run_image(cli: &Cli) -> Result<()> {
    if cli.name.is_some() {
        bail!("--name is only valid in font mode (directory of SVG icons)");
    }

    let (output, format) = resolve_image_output_and_format(cli)?;
    let tree = load_tree(&cli.input)?;

    match format {
        Format::Png => {
            let size = png_size(&tree, cli.size);
            let rgba = render_rgba(&tree, size)?;
            write_png(&output, &rgba, size)?;
        }
        Format::Ico => {
            let sizes = sizes_for(Format::Ico, cli.size);
            write_ico(&output, &tree, &sizes)?;
        }
        Format::Icns => {
            let sizes = sizes_for(Format::Icns, cli.size);
            write_icns(&output, &tree, &sizes)?;
        }
        Format::Ttf | Format::Woff | Format::Woff2 => unreachable!(),
    }

    println!("{} -> {} ({})", cli.input.display(), output.display(), format.as_str());
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    if !cli.input.exists() {
        bail!("input not found: {}", cli.input.display());
    }

    if cli.input.is_dir() {
        run_font(&cli)
    } else if cli.input.is_file() {
        run_image(&cli)
    } else {
        bail!("input is neither a file nor a directory: {}", cli.input.display());
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
