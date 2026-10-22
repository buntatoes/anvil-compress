# flux-compress

[![Release](https://img.shields.io/badge/release-v.01-e8c37a)](https://github.com/buntatoes/flux-compress/releases/tag/v.01)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](./LICENSE)
[![Edition](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://github.com/buntatoes/flux-compress)

`flux-compress` is a standalone, zero-dependency Rust library implementing **DEFLATE** (RFC 1951), **zlib** (RFC 1950), and **gzip** (RFC 1952) compression, decompression, and checksum verification (**Adler-32** and **CRC-32**).

---

## Features

- **Raw DEFLATE (`decompress_deflate` / `compress_deflate`)**:
  - Uncompressed stored blocks (`BTYPE = 00`)
  - Fixed Huffman blocks (`BTYPE = 01`)
  - Dynamic Huffman code trees (`BTYPE = 10`)
  - 32 KiB LZ77 sliding-window back-reference matching and reconstruction
- **zlib Framing (`decompress_zlib` / `compress_zlib`)**:
  - CMF/FLG header validation and big-endian **Adler-32** integrity verification
- **gzip Framing (`decompress_gzip` / `compress_gzip`)**:
  - Header parsing (`FEXTRA`, `FNAME`, `FCOMMENT`, `FHCRC`) and little-endian **CRC-32** + `ISIZE` trailer verification
- **Zero External Dependencies**: Pure safe Rust with no third-party crates.

---

## Quick Start

```rust
use flux_compress::{compress_gzip, compress_zlib, decompress_gzip, decompress_zlib};

let payload = b"Flux compression stream payload";

// zlib round-trip
let zlib_bytes = compress_zlib(payload);
let restored = decompress_zlib(&zlib_bytes).expect("valid zlib stream");
assert_eq!(restored, payload);

// gzip round-trip
let gzip_bytes = compress_gzip(payload);
let restored_gz = decompress_gzip(&gzip_bytes).expect("valid gzip stream");
assert_eq!(restored_gz, payload);
```

---

## API Overview

| Function / Type | Description |
| :--- | :--- |
| `decompress_deflate(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses a raw RFC 1951 DEFLATE bitstream. |
| `compress_deflate(&[u8]) -> Vec<u8>` | Compresses a byte slice into a raw DEFLATE bitstream using LZ77 + fixed Huffman encoding. |
| `decompress_zlib(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses an RFC 1950 zlib stream and verifies its Adler-32 trailer. |
| `compress_zlib(&[u8]) -> Vec<u8>` | Compresses a byte slice into an RFC 1950 zlib container. |
| `decompress_gzip(&[u8]) -> Result<Vec<u8>, CompressError>` | Decompresses an RFC 1952 gzip stream and verifies its CRC-32 and `ISIZE` trailer. |
| `compress_gzip(&[u8]) -> Vec<u8>` | Compresses a byte slice into an RFC 1952 gzip container. |
| `adler32(&[u8]) -> u32` | Computes the Adler-32 checksum of a byte slice. |
| `crc32(&[u8]) -> u32` | Computes the IEEE 802.3 CRC-32 checksum of a byte slice. |

---

## Building and Testing

```powershell
cargo test
```

---

## License

Licensed under the Mozilla Public License Version 2.0 ([`MPL-2.0`](./LICENSE)).

## Author

**Buntos**
