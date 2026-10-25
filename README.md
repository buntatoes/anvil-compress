<div align="center">
  <h1>anvil-compress</h1>
  <p>DEFLATE, zlib, and gzip.</p>
  <p>
    <img src="https://img.shields.io/badge/release-v.03-e8c37a?style=flat-square" alt="v.03" />
    <img src="https://img.shields.io/badge/status-Proof_of_Concept-d4a359?style=flat-square" alt="Proof of Concept" />
    <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue?style=flat-square" alt="MPL-2.0" /></a>
    <a href="./SECURITY.md"><img src="https://img.shields.io/badge/security-policy-2ea44f?style=flat-square" alt="Security" /></a>
    <img src="https://img.shields.io/badge/rust-2024-orange?style=flat-square" alt="Rust 2024" />
  </p>
</div>

Version 0.3.0.

The browser uses this for HTTP bodies and the profile file. [anvil-image](https://github.com/buntatoes/anvil-image) uses it for PNG and WebP, and uses `crc32` to check PNG chunks. [anvil-video](https://github.com/buntatoes/anvil-video) compresses frame data with it.

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

The browser is [anvil-browser](https://github.com/buntatoes/anvil-browser). These libraries sit next to it. None of them come from crates.io.

| Repository | What it is |
| --- | --- |
| [anvil-cipher](https://github.com/buntatoes/anvil-cipher) | AES-128, AES-256, CTR, AES-256-GCM |
| [anvil-compress](https://github.com/buntatoes/anvil-compress) | DEFLATE, zlib, gzip |
| [anvil-image](https://github.com/buntatoes/anvil-image) | PNG, JPEG, GIF, WebP, ICO, BMP, PPM |
| [anvil-json](https://github.com/buntatoes/anvil-json) | JSON |
| [anvil-ttf](https://github.com/buntatoes/anvil-ttf) | TrueType and OpenType (`glyf`, `CFF `, `kern`, `GPOS`) |
| [anvil-gpu](https://github.com/buntatoes/anvil-gpu) | Software drawing of boxes, shadows, gradients, and NV12 |
| [anvil-video](https://github.com/buntatoes/anvil-video) | CPU frames and PCM/ADPCM audio in a small MP4 container |

## License

MPL-2.0. Copyright (c) 2026 Buntos.
