# Third-party notices

Flick itself is MIT-licensed (see [LICENSE](LICENSE)). Flick does **not** convert
files on its own — it plans and orchestrates conversions by invoking well-known,
separately-installed command-line tools, each as its **own child process**.

Flick never links any of these tools into its own binary. They are discovered on
your `PATH` at runtime and executed as external processes. This keeps Flick's MIT
code cleanly separated from tools that ship under copyleft licenses (notably
Ghostscript under AGPL and some ffmpeg builds under GPL): Flick is an independent
program that *calls* them, not a derivative work that *includes* them. If you
redistribute a build of Flick that bundles any of these tools, you are responsible
for complying with that tool's license for the copy you bundle.

| Tool | Used for | Typical license | Project |
|------|----------|-----------------|---------|
| ImageMagick (`magick`) | Image formats, image→PDF, resize/compress/rotate | ImageMagick License (Apache-2.0-style) | https://imagemagick.org |
| FFmpeg (`ffmpeg`) | Audio & video formats, extract audio, mute, poster frame | LGPL-2.1+ or GPL (build-dependent) | https://ffmpeg.org |
| LibreOffice (`soffice`) | Office documents, spreadsheets, slides → PDF | MPL-2.0 | https://www.libreoffice.org |
| Pandoc (`pandoc`) | Markup/text/ebook conversions | GPL-2.0+ | https://pandoc.org |
| Poppler (`pdftoppm`) | PDF → raster images | GPL-2.0+ | https://poppler.freedesktop.org |
| MuPDF (`mutool`) | PDF → text | AGPL-3.0 | https://mupdf.com |
| qpdf (`qpdf`) | PDF split/merge/linearize | Apache-2.0 | https://qpdf.sourceforge.io |
| Ghostscript (`gs`) | PDF compression | AGPL-3.0 | https://www.ghostscript.com |
| 7-Zip (`7z`) | Archive create/extract | LGPL-2.1 (+ unRAR restriction) | https://www.7-zip.org |

## Build-time dependencies

| Dependency | License |
|------------|---------|
| Tauri (Rust + WRY/TAO) | MIT / Apache-2.0 |
| tauri-plugin-global-shortcut | MIT / Apache-2.0 |
| objc2, objc2-app-kit, objc2-foundation (AppKit bindings) | MIT |
| serde | MIT / Apache-2.0 |

Optional engines Flick can download on demand (pinned by URL and SHA-256 in
`app/crates/flick-engine/engines.json`; each runs as a separate process): FFmpeg 9.0.2 static
build (GPL-3.0, ffmpeg.martin-riedl.de), Pandoc 3.12.1 (GPL-2.0+), qpdf 12.4.2 (Apache-2.0),
7-Zip 23.01 console (LGPL-2.1 + unRAR restriction), LibreOffice 26.8.1 (MPL-2.0).
Built-in macOS tools used without any download: sips, afconvert, avconvert, textutil, cupsfilter,
PDFKit (via osascript), zip/tar — part of macOS.

The exact license of your FFmpeg build depends on how it was compiled (e.g. with
`--enable-gpl`). The Homebrew `ffmpeg` formula is GPL. Verify with `ffmpeg -version`.
