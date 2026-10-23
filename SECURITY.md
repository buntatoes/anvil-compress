# Security Policy

## Proof of Concept (PoC) Notice

`flux-compress` is a standalone, zero-dependency **Proof of Concept (PoC)** compression and bounded decompression library (`DEFLATE`, `zlib`, `gzip`) authored from scratch by **Buntos** as part of the [**Flux Browser**](https://github.com/buntatoes/flux-browser) engine ecosystem.

---

## Supported Versions

| Version | Supported |
| :--- | :---: |
| `v.02` (Latest) | Yes |
| `< v.02` | No |

---

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues or pull requests.

- **GitHub Private Vulnerability Reporting (Recommended)**: [`https://github.com/buntatoes/flux-compress/security/advisories/new`](https://github.com/buntatoes/flux-compress/security/advisories/new)
- **Central Ecosystem Security Portal**: [`https://github.com/buntatoes/flux-browser/security/advisories/new`](https://github.com/buntatoes/flux-browser/security/advisories/new)
- **Author Attribution**: **Buntos** ([@buntatoes](https://github.com/buntatoes) · `Buntos <324143321+buntatoes@users.noreply.github.com>`)

*(Note: `324143321+buntatoes@users.noreply.github.com` is a GitHub privacy `noreply` commit-attribution address and does not accept direct inbound email; please use GitHub Private Vulnerability Reporting above to contact the maintainer privately.)*

---

## Security Controls in `flux-compress`

- **Decompression-Bomb Guard (`DEFAULT_MAX_DECOMPRESSED_BYTES = 32 MiB`)**:
  - `decompress_deflate_bounded`, `decompress_zlib_bounded`, and `decompress_gzip_bounded` enforce hard output byte caps during LZ77 sliding-window expansion and uncompressed block reads (`CompressError::OutputTooLarge`).
- **Integrity Verification**:
  - Enforces big-endian **Adler-32** checksums on RFC 1950 `zlib` streams and little-endian **CRC-32** + `ISIZE` length trailers on RFC 1952 `gzip` streams.
- **Zero Unsafe Code & Zero Third-Party Crates**:
  - Implemented in 100% safe Rust with deterministic bounds checks on all bitstream reads and Huffman table lookups.
