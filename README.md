# svgc

Simple CLI that converts SVG files to **PNG**, **ICO**, or **ICNS**, or packs a directory of SVG icons into an icon font (**TTF** / **WOFF** / **WOFF2**).

Font mode mirrors the core of [svgtofont](https://github.com/jaywcjlove/svgtofont) (SVG icons → webfont). CSS, HTML preview, EOT, and SVG-font outputs are not generated.

## Usage

### Image (single SVG file)

```text
svgc <input.svg> [output]
```

Format is taken from the output extension. If you omit the output path, `svgc` writes `<input>.png` beside the source.

```powershell
svgc icon.svg
svgc icon.svg icon.png
svgc icon.svg icon.ico
svgc icon.svg icon.icns
svgc icon.svg -o out.png --size 512
```

### Icon font (directory of SVGs)

```text
svgc <icons-dir> [output-dir]
```

Collects `*.svg` in the directory and writes `<name>.ttf`, `<name>.woff`, and `<name>.woff2` (unless `-f` selects one format).

```powershell
svgc icons/
svgc icons/ -o dist/
svgc icons/ -o dist/ --name myicons
svgc icons/ -f woff2
```

| Flag | Meaning |
| --- | --- |
| `-o`, `--output` | Output path (image file, or font output directory) |
| `-f`, `--format` | `png` / `ico` / `icns` (file) or `ttf` / `woff` / `woff2` (directory; default all three) |
| `--name` | Font family / base file name (font mode; default: directory name) |
| `-s`, `--size` | Pixel size for PNG, or the single size embedded in ICO/ICNS |

Without `--size`, PNG uses the SVG viewport size (minimum 1). ICO embeds 16–256 px layers; ICNS embeds the common Apple sizes up to 1024 px.

## Build

Requires a Rust toolchain targeting Windows (`x86_64-pc-windows-msvc`) and the MSVC C++ tools.

```powershell
.\build.ps1
```

Release binary: `target\release\svgc.exe`. Switches: `-Dev` (debug), `-Target x86_64-pc-windows-msvc`.

Or with cargo:

```powershell
cargo build --release
```

## License

MIT. See `LICENSE`.
