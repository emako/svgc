# svgc

Simple CLI that converts SVG files to **PNG**, **ICO**, or **ICNS**.

## Usage

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

| Flag | Meaning |
| --- | --- |
| `-o`, `--output` | Output path (same as the optional positional argument) |
| `-f`, `--format` | `png`, `ico`, or `icns` when the path has no usable extension |
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
