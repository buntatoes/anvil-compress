# flux-compress

DEFLATE, zlib, and gzip. Version 0.2.0.

The browser uses this for HTTP bodies and the profile file. [flux-image](https://github.com/buntatoes/flux-image) uses it for PNG and WebP, and uses `crc32` to check PNG chunks. [flux-video](https://github.com/buntatoes/flux-video) compresses frame data with it.

## What it does

- `compress_deflate`, `compress_zlib`, `compress_gzip`, and the matching decompress functions.
- The `*_bounded` decompress functions stop if the output would pass the caller's limit. The default limit is 32 MiB (`DEFAULT_MAX_DECOMPRESSED_BYTES`).
- zlib streams are checked with Adler-32. gzip streams are checked with CRC-32 and the ISIZE trailer.
- `crc32` and `adler32` are public.

## What it does not do

- It does not call zlib, libdeflate, or any other compression library.

## Build

```
cargo test
```

Rust 2024. No crates.io dependencies.

## The other repositories

The browser is [flux-browser](https://github.com/buntatoes/flux-browser). These libraries sit next to it. None of them come from crates.io.

| Repository | What it is |
| --- | --- |
| [flux-cipher](https://github.com/buntatoes/flux-cipher) | AES-128, AES-256, CTR, AES-256-GCM |
| [flux-compress](https://github.com/buntatoes/flux-compress) | DEFLATE, zlib, gzip |
| [flux-image](https://github.com/buntatoes/flux-image) | PNG, JPEG, GIF, WebP, ICO, BMP, PPM |
| [flux-json](https://github.com/buntatoes/flux-json) | JSON |
| [flux-ttf](https://github.com/buntatoes/flux-ttf) | TrueType and OpenType |
| [flux-gpu](https://github.com/buntatoes/flux-gpu) | CPU drawing of boxes, shadows, and NV12 |
| [flux-video](https://github.com/buntatoes/flux-video) | CPU frames in a small MP4 container |

## License

MPL-2.0. Copyright (c) 2026 Buntos.
