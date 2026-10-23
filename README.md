<div align="center">
  <h1>flux-compress</h1>
  <p><strong>Zero-Dependency DEFLATE, zlib (Adler-32), and gzip (CRC-32) Compression &amp; Bounded Decompression Library (Proof of Concept)</strong></p>
  <p>
    <a href="https://github.com/buntatoes/flux-compress/releases/tag/v.02"><img src="https://img.shields.io/badge/release-v.02-e8c37a?style=flat-square" alt="Release v.02" /></a>
    <img src="https://img.shields.io/badge/status-Proof_of_Concept_(PoC)-d4a359?style=flat-square" alt="Proof of Concept" />
    <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue?style=flat-square" alt="License MPL-2.0" /></a>
    <a href="https://github.com/buntatoes/flux-compress"><img src="https://img.shields.io/badge/rust-2024_edition-orange?style=flat-square" alt="Rust 2024 Edition" /></a>
  </p>
</div>

> [!NOTE]
> **Proof of Concept (PoC) Notice**: `flux-compress` is a standalone, zero-dependency **Proof of Concept** compression and bounded decompression library authored from scratch by **Buntos** as part of the [**Flux Browser**](https://github.com/buntatoes/flux-browser) engine ecosystem. It demonstrates a self-contained implementation of **DEFLATE** (RFC 1951), **zlib** (RFC 1950), and **gzip** (RFC 1952) with built-in decompression-bomb protection (`DEFAULT_MAX_DECOMPRESSED_BYTES = 32 MiB`) and checksum verification (**Adler-32** and **CRC-32**) using zero third-party crates.

---

## Features (`v.02`)

- **Raw DEFLATE (`decompress_deflate` / `decompress_deflate_bounded` / `compress_deflate`)**:
  - Uncompressed stored blocks (`BTYPE = 00`)
  - Fixed Huffman blocks (`BTYPE = 01`)
  - Dynamic Huffman code trees (`BTYPE = 10`)
  - 32 KiB LZ77 sliding-window back-reference matching and reconstruction
- **Decompression-Bomb Guard (`DEFAULT_MAX_DECOMPRESSED_BYTES = 32 MiB`)**:
  - Enforces strict output byte caps during LZ77 expansion and stored-block reads (`CompressError::OutputTooLarge`), preventing adversarial streams from exhausting memory.
- **zlib Framing (`decompress_zlib` / `decompress_zlib_bounded` / `compress_zlib`)**:
  - CMF/FLG header validation and big-endian **Adler-32** integrity verification
- **gzip Framing (`decompress_gzip` / `decompress_gzip_bounded` / `compress_gzip`)**:
  - Header parsing (`FEXTRA`, `FNAME`, `FCOMMENT`, `FHCRC`) and little-endian **CRC-32** + `ISIZE` trailer verification
- **Zero External Dependencies**: Pure safe Rust with no third-party crates.

---

## Quick Start

```rust
use flux_compress::{
    compress_gzip, compress_zlib, decompress_gzip_bounded, decompress_zlib,
};

let payload = b"Flux compression stream payload";

// zlib round-trip
let zlib_bytes = compress_zlib(payload);
let restored = decompress_zlib(&zlib_bytes).expect("valid zlib stream");
assert_eq!(restored, payload);

// Bounded gzip decompression with explicit byte ceiling
let gzip_bytes = compress_gzip(payload);
let restored_gz = decompress_gzip_bounded(&gzip_bytes, 64 * 1024).expect("within cap");
assert_eq!(restored_gz, payload);
```

---

## API Overview

| Function / Type | Description |
| :--- | :--- |
| `decompress_deflate(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses a raw RFC 1951 DEFLATE bitstream (32 MiB cap). |
| `decompress_deflate_bounded(&[u8], usize) -> Result<Vec<u8>, CompressError>` | Decompresses a raw DEFLATE bitstream with a custom output byte cap. |
| `compress_deflate(&[u8]) -> Vec<u8>` | Compresses a byte slice into a raw DEFLATE bitstream using LZ77 + fixed Huffman encoding. |
| `decompress_zlib(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses an RFC 1950 zlib stream and verifies its Adler-32 trailer (32 MiB cap). |
| `decompress_zlib_bounded(&[u8], usize) -> Result<Vec<u8>, CompressError>` | Decompresses a zlib stream with a custom output byte cap. |
| `compress_zlib(&[u8]) -> Vec<u8>` | Compresses a byte slice into an RFC 1950 zlib container. |
| `decompress_gzip(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses an RFC 1952 gzip stream and verifies its CRC-32 and `ISIZE` trailer (32 MiB cap). |
| `decompress_gzip_bounded(&[u8], usize) -> Result<Vec<u8>, CompressError>` | Decompresses a gzip stream with a custom output byte cap. |
| `compress_gzip(&[u8]) -> Vec<u8>` | Compresses a byte slice into an RFC 1952 gzip container. |
| `adler32(&[u8]) -> u32` | Computes the Adler-32 checksum of a byte slice. |
| `crc32(&[u8]) -> u32` | Computes the IEEE 802.3 CRC-32 checksum of a byte slice. |

---

## Building and Testing

```powershell
cargo test
```

---

## Security Policy

See [`SECURITY.md`](./SECURITY.md) for supported versions, decompression-bomb security controls, and vulnerability reporting instructions.

## License

Licensed under the Mozilla Public License Version 2.0 ([`MPL-2.0`](./LICENSE)).

## Author

**Buntos** ([@buntatoes](https://github.com/buntatoes))
