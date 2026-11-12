//! svgc — convert SVG to PNG, ICO, or ICNS.

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

/// Convert SVG to PNG, ICO, or ICNS.
#[derive(Debug, Parser)]
#[command(
    name = "svgc",
    version,
    about = "Convert SVG to PNG, ICO, or ICNS",
    arg_required_else_help = true,
    after_help = "Examples:\n  svgc icon.svg\n  svgc icon.svg icon.ico\n  svgc icon.svg -o out.icns --size 512"
)]
struct Cli {
    /// Input SVG file
    input: PathBuf,

    /// Output path (optional; defaults to <input>.png)
    #[arg(value_name = "OUTPUT")]
    output_pos: Option<PathBuf>,

    /// Output path (overrides the positional OUTPUT)
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Output format when the path has no usable extension
    #[arg(short, long, value_name = "FMT")]
    format: Option<Format>,

    /// Pixel size (PNG width/height; single size for ICO/ICNS)
    #[arg(short, long, value_name = "PX")]
    size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Ico,
    Icns,
}

impl Format {
    fn as_str(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Ico => "ico",
            Format::Icns => "icns",
        }
    }

    fn extension(self) -> &'static str {
        self.as_str()
    }
}

impl FromStr for Format {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "png" => Ok(Format::Png),
            "ico" => Ok(Format::Ico),
            "icns" => Ok(Format::Icns),
            other => bail!("unknown format '{other}' (use png, ico, or icns)"),
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

fn resolve_output_and_format(cli: &Cli) -> Result<(PathBuf, Format)> {
    let out = cli
        .output
        .clone()
        .or_else(|| cli.output_pos.clone())
        .unwrap_or_else(|| default_output(&cli.input, Format::Png));

    let format = if let Some(f) = cli.format {
        f
    } else if let Some(f) = format_from_path(&out) {
        f
    } else if cli.output.is_none() && cli.output_pos.is_none() {
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
        Format::Png => Vec::new(),
        Format::Ico => ICO_SIZES.to_vec(),
        Format::Icns => ICNS_SIZES.to_vec(),
    }
}

fn run(cli: Cli) -> Result<()> {
    if !cli.input.exists() {
        bail!("input not found: {}", cli.input.display());
    }

    let (output, format) = resolve_output_and_format(&cli)?;
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
    }

    println!("{} -> {} ({})", cli.input.display(), output.display(), format.as_str());
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
