<div align="center">
  <h1>Anvil Compress</h1>
  <p>Clean-room DEFLATE, zlib, and gzip compression and decompression in Go.</p>
  <p>
    <img src="https://img.shields.io/badge/release-v.04-e8c37a?style=flat-square" alt="v.04" />
    <img src="https://img.shields.io/badge/status-Proof_of_Concept-d4a359?style=flat-square" alt="Proof of Concept" />
    <a href="./LICENSE"><img src="https://img.shields.io/badge/license-Noncommercial_%28based_on_PolyForm_1.0.0%29_%7C_Paid_Commercial_%28Non--Compete%29-blue?style=flat-square" alt="Noncommercial (based on PolyForm 1.0.0) | Paid Commercial (Non-Compete)" /></a>
    <a href="./SECURITY.md"><img src="https://img.shields.io/badge/security-policy-2ea44f?style=flat-square" alt="Security" /></a>
    <img src="https://img.shields.io/badge/go-1.22%2B-00ADD8?style=flat-square" alt="Go 1.22+" />
  </p>
</div>

Version 0.4.0 (`v.04`).

The browser uses this for HTTP bodies and the profile file. [anvil-image](https://github.com/buntatoes/anvil-image) uses it for PNG and WebP, and uses `Crc32` to check PNG chunks. [anvil-video](https://github.com/buntatoes/anvil-video) compresses frame data with it. Written from scratch in Go (`github.com/buntatoes/anvil-compress`) without Go's `compress/*` or `hash/*` packages.

## What it does

- `DeflateCompress`, `ZlibCompress`, `GzipCompress`, and the matching decompress functions (`DeflateDecompress`, `ZlibDecompress`, `GzipDecompress`).
- The `*Bounded` decompress functions stop if the output would pass the caller's limit. The default limit is 32 MiB (`DefaultMaxDecompressedBytes`).
- `GzipDecompress` and `GzipDecompressBounded` support both single-member and concatenated multi-member gzip streams (RFC 1952 section 2.2).
- `DecompressAuto` inspects the header and automatically decompresses gzip (`0x1f 0x8b`), zlib (RFC 1950 header), or raw DEFLATE.
- zlib streams are checked with `Adler32`. gzip streams are checked with `Crc32` and the `ISIZE` trailer.

## What it does not do

- It does not call zlib, libdeflate, Go's `compress/*`, or any external compression library.

## Build

```
go vet ./...
go test -v ./...
```

Go 1.22 or newer (`CGO_ENABLED=0`). Zero external Go module dependencies.

## The other repositories

The browser is [anvil-browser](https://github.com/buntatoes/anvil-browser) and the page engine is [anvil-engine](https://github.com/buntatoes/anvil-engine), built alongside the nine companion libraries below.

| Repository | What it is |
| --- | --- |
| [anvil-cipher](https://github.com/buntatoes/anvil-cipher) | AES-128, AES-256, CTR, AES-256-GCM, SHA-256, HMAC-SHA256, HKDF-SHA256 |
| [anvil-compress](https://github.com/buntatoes/anvil-compress) | DEFLATE, zlib, gzip (Go) |
| [anvil-hpack](https://github.com/buntatoes/anvil-hpack) | HPACK header compression for HTTP/2 (Go) |
| [anvil-image](https://github.com/buntatoes/anvil-image) | PNG, JPEG, GIF, WebP (`VP8L` + `VP8 `), ICO, BMP, PPM |
| [anvil-json](https://github.com/buntatoes/anvil-json) | JSON & RFC 6901 JSON Pointer (Go) |
| [anvil-pqc](https://github.com/buntatoes/anvil-pqc) | ML-KEM-768, X25519, X25519MLKEM768, SHA-3 |
| [anvil-ttf](https://github.com/buntatoes/anvil-ttf) | TrueType and OpenType (`glyf`, `CFF `, `kern`, `GPOS`) |
| [anvil-gpu](https://github.com/buntatoes/anvil-gpu) | Compositor passes for boxes, shadows, gradients, and NV12/I420, on the CPU or a GPU |
| [anvil-video](https://github.com/buntatoes/anvil-video) | CPU frames (AVID/ADCT) and PCM/ADPCM/FLAC/MDCT audio in MP4, fMP4, and WebM containers |

## License

Dual-licensed under **noncommercial terms based on the PolyForm Noncommercial License 1.0.0** and a **Paid Commercial License with a Non-Compete Clause**. Copyright (c) 2026 Buntato. See [LICENSE](./LICENSE).
