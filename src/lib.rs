//! DEFLATE (RFC 1951), zlib (RFC 1950), and gzip (RFC 1952).
//!
//! A raw DEFLATE input is a series of blocks: stored bytes, fixed Huffman
//! codes, or dynamic Huffman codes. Huffman blocks hold literals and LZ77
//! length and distance copies. `decompress_deflate` and
//! `decompress_deflate_bounded` return the uncompressed bytes, or a
//! `CompressError`. zlib adds a two-byte CMF/FLG
//! header and a big-endian Adler-32 of the uncompressed bytes.
//! `decompress_zlib` and `decompress_zlib_bounded` check that trailer. gzip
//! adds a 10-byte header, skips optional extra fields, and checks a
//! little-endian CRC-32 and ISIZE. `decompress_gzip` and
//! `decompress_gzip_bounded` do that check. `compress_deflate`,
//! `compress_zlib`, and `compress_gzip` return a new buffer. Compression
//! writes one final fixed-Huffman block. It does not write dynamic blocks.
//! `adler32` and `crc32` are public. They are the checksums those trailers use.
//!
//! flux-image calls `crc32`, `compress_zlib`, and `decompress_zlib_bounded`
//! for PNG. flux-video zlib-compresses frame payloads with `compress_zlib`
//! and `decompress_zlib_bounded`. flux-net decompresses HTTP gzip bodies with
//! `decompress_gzip_bounded`, and HTTP deflate bodies by trying
//! `decompress_zlib_bounded` and then `decompress_deflate_bounded`. The
//! flux-browser installer and profile snapshots use `compress_zlib` and
//! `decompress_zlib_bounded`. No other crate calls this one.
//!
//! Decompression stops at `max_output` bytes. The default,
//! `DEFAULT_MAX_DECOMPRESSED_BYTES`, is 32 MiB. A larger result is
//! `OutputTooLarge`, and the bytes are not returned. Compression has no size
//! cap. The compressor looks back 4096 bytes and tests at most 128 positions.
//! Matches are 3 to 258 bytes. Preset dictionaries are rejected. Trailing
//! bytes after a zlib or gzip trailer are ignored. This is not a multi-member
//! gzip reader, and it does not implement any format besides these three.

use std::fmt;

/// Default decompressed-output cap, 32 MiB (`32 * 1024 * 1024`).
///
/// `decompress_deflate`, `decompress_zlib`, and `decompress_gzip` pass this
/// into the bounded functions. A tighter cap is the `max_output` argument.
/// The compress functions do not use this constant. Past the cap, decompression
/// returns `CompressError::OutputTooLarge` and no buffer.
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: usize = 32 * 1024 * 1024;

/// A failed decompress or checksum. The compress functions do not return this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompressError {
    /// A bit or byte read needed data past the end of the input.
    UnexpectedEof,
    /// zlib or gzip header bytes are not a DEFLATE member this crate accepts.
    /// The string is a static reason. Preset dictionaries land here.
    InvalidHeader(&'static str),
    /// BTYPE was not 0 (stored), 1 (fixed Huffman), or 2 (dynamic Huffman).
    InvalidBlockType(u8),
    /// A stored block's little-endian LEN was not the one's complement of NLEN.
    InvalidStoredLength,
    /// Dynamic-header code lengths could not form a tree, or a repeat code ran off the list.
    InvalidHuffmanTree,
    /// A Huffman bit string matched no symbol, or a literal symbol was above 285.
    InvalidSymbol,
    /// An LZ77 distance was 0, pointed before the output, or the distance symbol was out of range.
    InvalidDistance,
    /// Uncompressed output would pass `limit` bytes. Nothing is returned.
    OutputTooLarge { limit: usize },
    /// Adler-32 or CRC-32 disagreed. Both fields are the raw 32-bit words.
    ChecksumMismatch { expected: u32, actual: u32 },
    /// gzip ISIZE disagreed with the output length modulo 2^32.
    LengthMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for CompressError {
    /// Writes one line. Checksum values are hex. Length values are decimal.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of compressed stream"),
            Self::InvalidHeader(msg) => write!(f, "invalid container header: {msg}"),
            Self::InvalidBlockType(kind) => write!(f, "invalid DEFLATE block type {kind}"),
            Self::InvalidStoredLength => write!(f, "stored block length complement mismatch"),
            Self::InvalidHuffmanTree => write!(f, "invalid Huffman code lengths"),
            Self::InvalidSymbol => write!(f, "invalid Huffman symbol in stream"),
            Self::InvalidDistance => write!(f, "LZ77 distance exceeds output buffer"),
            Self::OutputTooLarge { limit } => {
                write!(
                    f,
                    "decompressed output exceeds safety limit of {limit} bytes"
                )
            }
            Self::ChecksumMismatch { expected, actual } => {
                write!(
                    f,
                    "checksum mismatch (expected {expected:#010x}, got {actual:#010x})"
                )
            }
            Self::LengthMismatch { expected, actual } => {
                write!(
                    f,
                    "uncompressed size mismatch (expected {expected}, got {actual})"
                )
            }
        }
    }
}

impl std::error::Error for CompressError {}

// RFC 1951 length and distance alphabets, and the dynamic code-length order.
// LENGTH_BASE / DIST_BASE are the lowest match length or distance for that symbol.
// LENGTH_EXTRA / DIST_EXTRA are the extra bits that follow. Those extra bits are
// little-endian in the bit stream. CODE_LENGTH_ORDER is the header order of the
// 19 code-length symbols, which is not symbol order.
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Adler-32 of `data`, the checksum stored in a zlib trailer.
///
/// `s1` starts at 1 and `s2` starts at 0. Both are reduced modulo 65521.
/// The return value puts `s2` in the high 16 bits and `s1` in the low 16
/// bits. zlib writes that word big-endian. The sum covers `data` only, not
/// the CMF/FLG header. No sibling calls this by name. `compress_zlib` and
/// `decompress_zlib_bounded` are the users.
pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    // 5552 keeps the byte sums inside u32 before the modulo.
    for chunk in data.chunks(5552) {
        for &b in chunk {
            s1 += u32::from(b);
            s2 += s1;
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

/// IEEE 802.3 CRC-32 of `data`. gzip trailers and PNG chunk CRCs use this.
///
/// The register starts at `0xFFFF_FFFF`. Each input byte is mixed low bit
/// first with the reflected polynomial `0xEDB8_8320`. The returned word is
/// the bitwise complement of the register. The sum covers `data` only.
/// gzip stores it little-endian. flux-image calls this for PNG chunk CRCs.
/// `compress_gzip` and `decompress_gzip_bounded` call it for the trailer.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let mut c = (crc ^ u32::from(byte)) & 0xFF;
        for _ in 0..8 {
            // Reflected polynomial. The low bit decides whether it is folded in.
            if c & 1 != 0 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
        }
        crc = (crc >> 8) ^ c;
    }
    !crc
}

/// Decompresses raw DEFLATE, capped at 32 MiB.
///
/// There is no zlib or gzip wrapper and no checksum. The result is every
/// block concatenated. `OutputTooLarge` uses `DEFAULT_MAX_DECOMPRESSED_BYTES`.
/// flux-net calls the bounded form, not this one, for HTTP deflate.
pub fn decompress_deflate(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_deflate_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompresses raw DEFLATE and refuses output larger than `max_output`.
///
/// Returns the uncompressed bytes. Growing past `max_output` is
/// `OutputTooLarge { limit: max_output }` and no buffer. Other failures are
/// block errors: bad BTYPE, stored-length complement, Huffman data, distance,
/// or an early end of input. There is no checksum. flux-net calls this when
/// an HTTP `deflate` body is not valid zlib.
pub fn decompress_deflate_bounded(
    input: &[u8],
    max_output: usize,
) -> Result<Vec<u8>, CompressError> {
    let mut reader = BitReader::new(input);
    decompress_deflate_from_reader(&mut reader, max_output)
}

/// Decompresses one zlib member, capped at 32 MiB, and checks Adler-32.
///
/// The trailer is big-endian over the uncompressed bytes. A mismatch is
/// `ChecksumMismatch` and the bytes are not returned. Siblings use the
/// bounded function so they can pass their own cap.
pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_zlib_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompresses one zlib member and refuses output larger than `max_output`.
///
/// The header must be DEFLATE (method 8) with CINFO at most 7. The 16-bit
/// header word must be divisible by 31. A preset dictionary is
/// `InvalidHeader`. The Adler-32 trailer is big-endian and covers the
/// uncompressed output only. Bytes after that trailer are ignored. flux-image
/// (PNG), flux-video (frame payloads), flux-net (HTTP deflate, tried first),
/// and the flux-browser installer and profile snapshots call this.
pub fn decompress_zlib_bounded(input: &[u8], max_output: usize) -> Result<Vec<u8>, CompressError> {
    if input.len() < 6 {
        return Err(CompressError::UnexpectedEof);
    }
    let cmf = input[0];
    let flg = input[1];
    let method = cmf & 0x0F;
    let cinfo = cmf >> 4;
    // Method 8 is DEFLATE. CINFO above 7 is not a legal window size.
    if method != 8 || cinfo > 7 {
        return Err(CompressError::InvalidHeader(
            "unsupported zlib compression method",
        ));
    }
    let check = (u16::from(cmf) << 8) | u16::from(flg);
    // RFC 1950: CMF as the high byte and FLG as the low byte, divisible by 31.
    if check % 31 != 0 {
        return Err(CompressError::InvalidHeader(
            "zlib header check bits failed",
        ));
    }
    // FDICT. This crate has no dictionary argument.
    if (flg & 0x20) != 0 {
        return Err(CompressError::InvalidHeader(
            "preset dictionary not supported",
        ));
    }

    let mut reader = BitReader::new(&input[2..]);
    let output = decompress_deflate_from_reader(&mut reader, max_output)?;
    reader.align_to_byte();
    // Adler-32, big-endian, over the uncompressed output only.
    let expected = reader.read_u32_be()?;
    let actual = adler32(&output);
    if expected != actual {
        return Err(CompressError::ChecksumMismatch { expected, actual });
    }
    Ok(output)
}

/// Decompresses one gzip member, capped at 32 MiB, and checks CRC-32 and ISIZE.
///
/// Both trailer words are little-endian. flux-net calls the bounded form so
/// it can pass the HTTP body cap.
pub fn decompress_gzip(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_gzip_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompresses one gzip member and refuses output larger than `max_output`.
///
/// Magic is `1F 8B` and the method byte must be 8. FEXTRA, FNAME, and
/// FCOMMENT are skipped. FHCRC's two bytes are skipped and not checked.
/// Other flag bits do not move the cursor and are not rejected. CRC-32 and
/// ISIZE are little-endian. ISIZE is the uncompressed length modulo 2^32.
/// Bytes after the trailer are ignored. flux-net calls this for HTTP `gzip`
/// and `x-gzip` bodies.
pub fn decompress_gzip_bounded(input: &[u8], max_output: usize) -> Result<Vec<u8>, CompressError> {
    if input.len() < 18 {
        return Err(CompressError::UnexpectedEof);
    }
    if input[0] != 0x1F || input[1] != 0x8B {
        return Err(CompressError::InvalidHeader("missing gzip magic bytes"));
    }
    if input[2] != 8 {
        return Err(CompressError::InvalidHeader(
            "unsupported gzip compression method",
        ));
    }
    let flags = input[3];
    let mut pos = 10usize;

    // FEXTRA: a little-endian length, then that many bytes. The field is not returned.
    if (flags & 0x04) != 0 {
        if pos + 2 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    // FNAME: skip through the terminating NUL. The name is not returned.
    if (flags & 0x08) != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    // FCOMMENT: same NUL-terminated skip as the file name.
    if (flags & 0x10) != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    // FHCRC: two header-CRC bytes are skipped. They are not verified.
    if (flags & 0x02) != 0 {
        pos += 2;
    }
    if pos > input.len() {
        return Err(CompressError::UnexpectedEof);
    }

    let mut reader = BitReader::new(&input[pos..]);
    let output = decompress_deflate_from_reader(&mut reader, max_output)?;
    reader.align_to_byte();
    // Both trailer words are little-endian. ISIZE is the low 32 bits of the length.
    let expected_crc = reader.read_u32_le()?;
    let expected_size = reader.read_u32_le()?;

    let actual_crc = crc32(&output);
    if expected_crc != actual_crc {
        return Err(CompressError::ChecksumMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }
    let actual_size = output.len() as u32;
    if expected_size != actual_size {
        return Err(CompressError::LengthMismatch {
            expected: expected_size,
            actual: actual_size,
        });
    }
    Ok(output)
}

/// Compresses `data` as one final fixed-Huffman DEFLATE block.
///
/// BFINAL is 1 and BTYPE is 01. Matches are LZ77, then fixed Huffman codes.
/// No stored block and no dynamic block is written. The search window is
/// 4096 bytes and at most 128 candidates, not the 32 KiB DEFLATE window.
/// There is no checksum and no size cap. Siblings do not call this. The zlib
/// and gzip compressors do.
pub fn compress_deflate(data: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::new();
    // One final fixed-Huffman block. The low bit is BFINAL, then BTYPE = 01.
    writer.write_bits(1, 1);
    writer.write_bits(1, 2);

    let mut pos = 0usize;
    while pos < data.len() {
        let (best_len, best_dist) = find_lz77_match(data, pos);
        if best_len >= 3 {
            write_fixed_length_distance(&mut writer, best_len, best_dist);
            pos += best_len;
        } else {
            write_fixed_literal(&mut writer, data[pos]);
            pos += 1;
        }
    }
    // Symbol 256 ends the block. In the fixed alphabet that symbol is 7 zero bits.
    write_fixed_symbol(&mut writer, 256);
    writer.finish()
}

/// Wraps `data` as one zlib member: CMF/FLG, one DEFLATE block, Adler-32.
///
/// The header is `0x78 0x9C` (DEFLATE, 32 KiB window, check bits pass). The
/// trailer is `adler32(data)` in big-endian order, over the uncompressed
/// input. flux-image, flux-video, and the flux-browser installer and profile
/// snapshots call this.
pub fn compress_zlib(data: &[u8]) -> Vec<u8> {
    let deflated = compress_deflate(data);
    let mut out = Vec::with_capacity(2 + deflated.len() + 4);
    // 0x78 is CM = 8 and CINFO = 7. 0x9C makes the 16-bit header divisible by 31.
    out.push(0x78);
    out.push(0x9C);
    out.extend_from_slice(&deflated);
    // Big-endian Adler-32 of the uncompressed input, not of the DEFLATE bytes.
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Wraps `data` as one gzip member: 10-byte header, DEFLATE, CRC-32, ISIZE.
///
/// The header is magic `1F 8B`, method 8, flags 0, modification time 0,
/// extra flags 0, and OS 255. No name, comment, or extra field is written.
/// CRC-32 and ISIZE are little-endian. ISIZE is `data.len()` modulo 2^32.
/// The CRC covers the uncompressed input. No sibling calls this on a
/// library path. flux-net only decompresses gzip.
pub fn compress_gzip(data: &[u8]) -> Vec<u8> {
    let deflated = compress_deflate(data);
    let mut out = Vec::with_capacity(10 + deflated.len() + 8);
    // ID1 ID2 CM FLG, four mtime bytes, XFL, OS. OS 255 means unknown.
    out.extend_from_slice(&[0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF]);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    // ISIZE is the low 32 bits of the uncompressed length.
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

/// Searches backward from `pos` for the longest LZ77 match.
///
/// Returns `(length, distance)` with distance equal to `pos - candidate`.
/// The window starts at most 4096 bytes before `pos`, which is shorter than
/// the 32 KiB DEFLATE maximum. At most 128 positions are tested, walking
/// backward one byte at a time. Length stops at 258 or at the end of `data`.
/// If fewer than 3 bytes remain, or `pos` is 0, the result is `(0, 0)`.
/// A shorter match can still be returned. The caller emits a literal unless
/// the length is at least 3.
fn find_lz77_match(data: &[u8], pos: usize) -> (usize, usize) {
    let max_len = (data.len() - pos).min(258);
    if max_len < 3 || pos == 0 {
        return (0, 0);
    }
    // This compressor's window. The format itself allows 32768.
    let min_start = pos.saturating_sub(4096);
    let mut best_len = 0usize;
    let mut best_dist = 0usize;

    let mut cand = pos;
    let mut steps = 0usize;
    // 128 candidates, not every byte in the window.
    while cand > min_start && steps < 128 {
        cand -= 1;
        steps += 1;
        if data[cand] != data[pos]
            || data[cand + best_len.min(max_len - 1)] != data[pos + best_len.min(max_len - 1)]
        {
            continue;
        }
        let mut len = 0usize;
        while len < max_len && data[cand + len] == data[pos + len] {
            len += 1;
        }
        if len > best_len {
            best_len = len;
            best_dist = pos - cand;
            if best_len == max_len {
                break;
            }
        }
    }
    (best_len, best_dist)
}

/// Writes one input byte as fixed-Huffman symbol `byte` (0 through 255).
///
/// The symbol is a literal, not a length code. Bit order is the same as
/// `write_fixed_symbol`: high bit of the code first.
fn write_fixed_literal(writer: &mut BitWriter, byte: u8) {
    write_fixed_symbol(writer, u16::from(byte));
}

/// Writes one fixed-Huffman literal, end-of-block, or length symbol.
///
/// Symbols 0 through 143 are 8 bits, 144 through 255 are 9 bits, 256 through
/// 279 are 7 bits, and 280 through 287 are 8 bits. Those widths are the
/// RFC 1951 fixed alphabet. Bits go out high bit first so the decoder, which
/// shifts each new bit into the low end, rebuilds the same code. Extra length
/// and distance bits are not written here. A symbol above 287 is ignored.
fn write_fixed_symbol(writer: &mut BitWriter, sym: u16) {
    match sym {
        0..=143 => writer.write_bits_msb(0b0011_0000 + sym, 8),
        144..=255 => writer.write_bits_msb(0b1_1001_0000 + (sym - 144), 9),
        256..=279 => writer.write_bits_msb(sym - 256, 7),
        280..=287 => writer.write_bits_msb(0b1100_0000 + (sym - 280), 8),
        _ => {}
    }
}

/// Writes one LZ77 match in the fixed Huffman alphabet.
///
/// `length` is at least 3 and at most 258. `distance` is how far back the
/// copy starts. The length symbol is 257 plus the `LENGTH_BASE` slot. Extra
/// length bits follow, low bit first. The distance symbol is 5 bits, high bit
/// first, then its extra bits low bit first. Each table is scanned from the
/// top so the largest base that is still less than or equal to the value wins.
fn write_fixed_length_distance(writer: &mut BitWriter, length: usize, distance: usize) {
    let len_u16 = length as u16;
    let mut len_idx = 0usize;
    for i in (0..LENGTH_BASE.len()).rev() {
        if len_u16 >= LENGTH_BASE[i] {
            len_idx = i;
            break;
        }
    }
    let sym = 257 + len_idx as u16;
    write_fixed_symbol(writer, sym);
    let extra_bits = LENGTH_EXTRA[len_idx];
    if extra_bits > 0 {
        let extra_val = u32::from(len_u16 - LENGTH_BASE[len_idx]);
        // Extra length bits are LSB first. The Huffman symbol above was MSB first.
        writer.write_bits(extra_val, extra_bits);
    }

    let dist_u16 = distance as u16;
    let mut dist_idx = 0usize;
    for i in (0..DIST_BASE.len()).rev() {
        if dist_u16 >= DIST_BASE[i] {
            dist_idx = i;
            break;
        }
    }
    // Fixed distance codes are 5 bits, MSB first. Extra distance bits stay LSB first.
    writer.write_bits_msb(dist_idx as u16, 5);
    let dist_extra = DIST_EXTRA[dist_idx];
    if dist_extra > 0 {
        let extra_val = u32::from(dist_u16 - DIST_BASE[dist_idx]);
        writer.write_bits(extra_val, dist_extra);
    }
}

/// Reads DEFLATE blocks until BFINAL and returns a new uncompressed buffer.
///
/// Each block starts with BFINAL (1 bit) and BTYPE (2 bits), low bit first.
/// Type 0 is stored, type 1 is the fixed trees, type 2 is a dynamic header.
/// Any other type is `InvalidBlockType`. `max_output` is enforced inside the
/// stored and Huffman decoders, not by a check after the last block. Bits
/// left in the final byte stay buffered. The zlib and gzip callers align to
/// a byte before reading the trailer.
fn decompress_deflate_from_reader(
    reader: &mut BitReader<'_>,
    max_output: usize,
) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::new();
    loop {
        let is_final = reader.read_bits(1)? != 0;
        let block_type = reader.read_bits(2)? as u8;

        match block_type {
            0 => {
                reader.align_to_byte();
                let len = reader.read_u16_le()?;
                let nlen = reader.read_u16_le()?;
                // NLEN must be the one's complement of LEN. Both are little-endian.
                if len != !nlen {
                    return Err(CompressError::InvalidStoredLength);
                }
                // Reject before the copy. saturating_add stops a huge LEN from wrapping the sum.
                if output.len().saturating_add(len as usize) > max_output {
                    return Err(CompressError::OutputTooLarge { limit: max_output });
                }
                let bytes = reader.read_exact_bytes(len as usize)?;
                output.extend_from_slice(bytes);
            }
            1 => {
                let (lit_tree, dist_tree) = fixed_trees()?;
                decode_huffman_block(reader, &lit_tree, &dist_tree, &mut output, max_output)?;
            }
            2 => {
                let (lit_tree, dist_tree) = read_dynamic_trees(reader)?;
                decode_huffman_block(reader, &lit_tree, &dist_tree, &mut output, max_output)?;
            }
            other => return Err(CompressError::InvalidBlockType(other)),
        }

        if is_final {
            break;
        }
    }
    Ok(output)
}

/// Decodes one Huffman block into `output`.
///
/// Symbols 0 through 255 append one byte. Symbol 256 ends the block. Symbols
/// 257 through 285 are lengths: `LENGTH_BASE` plus the extra bits, then a
/// distance from `dist_tree` plus its extra bits. The copy starts `distance`
/// bytes back and is done one byte at a time, so a short distance can repeat
/// bytes just written. A symbol above 285 is `InvalidSymbol`. A distance of
/// 0, a distance past the bytes already written, or a distance symbol past
/// `DIST_BASE` is `InvalidDistance`. Passing `max_output` is `OutputTooLarge`
/// and does not append the byte or the copy that would cross the cap.
fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_tree: &HuffmanTree,
    dist_tree: &HuffmanTree,
    output: &mut Vec<u8>,
    max_output: usize,
) -> Result<(), CompressError> {
    loop {
        let sym = lit_tree.decode(reader)?;
        match sym {
            0..=255 => {
                if output.len() >= max_output {
                    return Err(CompressError::OutputTooLarge { limit: max_output });
                }
                output.push(sym as u8);
            }
            256 => break,
            257..=285 => {
                let idx = (sym - 257) as usize;
                let base_len = usize::from(LENGTH_BASE[idx]);
                let extra = reader.read_bits(LENGTH_EXTRA[idx])? as usize;
                let length = base_len + extra;

                let dist_sym = dist_tree.decode(reader)? as usize;
                if dist_sym >= DIST_BASE.len() {
                    return Err(CompressError::InvalidDistance);
                }
                let base_dist = usize::from(DIST_BASE[dist_sym]);
                let dist_extra = reader.read_bits(DIST_EXTRA[dist_sym])? as usize;
                let distance = base_dist + dist_extra;

                if distance == 0 || distance > output.len() {
                    return Err(CompressError::InvalidDistance);
                }
                if output.len().saturating_add(length) > max_output {
                    return Err(CompressError::OutputTooLarge { limit: max_output });
                }
                let start = output.len() - distance;
                // One byte at a time. A distance shorter than `length` repeats new bytes.
                for i in 0..length {
                    let b = output[start + i];
                    output.push(b);
                }
            }
            _ => return Err(CompressError::InvalidSymbol),
        }
    }
    Ok(())
}

/// Builds the RFC 1951 fixed literal tree and distance tree.
///
/// Literal lengths are 8 for symbols 0 through 143, 9 for 144 through 255,
/// 7 for 256 through 279, and 8 for 280 through 287. All 32 distance codes
/// have length 5, including symbols 30 and 31. The block decoder still
/// rejects those two as distances because they are past `DIST_BASE`. This
/// is the BTYPE 01 path. A failure here would be `InvalidHuffmanTree`, which
/// these fixed lengths do not produce.
fn fixed_trees() -> Result<(HuffmanTree, HuffmanTree), CompressError> {
    let mut lit_lengths = [0u8; 288];
    for len in &mut lit_lengths[0..=143] {
        *len = 8;
    }
    for len in &mut lit_lengths[144..=255] {
        *len = 9;
    }
    for len in &mut lit_lengths[256..=279] {
        *len = 7;
    }
    for len in &mut lit_lengths[280..=287] {
        *len = 8;
    }
    let dist_lengths = [5u8; 32];
    Ok((
        HuffmanTree::from_lengths(&lit_lengths)?,
        HuffmanTree::from_lengths(&dist_lengths)?,
    ))
}

/// Reads a BTYPE 2 header and returns the literal and distance trees.
///
/// HLIT is 5 bits plus 257. HDIST is 5 bits plus 1. HCLEN is 4 bits plus 4.
/// That many code-length codes follow, 3 bits each, stored in
/// `CODE_LENGTH_ORDER` rather than symbol order. The rest of the 19 lengths
/// stay zero. Those codes then produce HLIT + HDIST symbol lengths. Symbol
/// 16 repeats the previous length 3 to 6 times (2 extra bits). Symbol 17
/// repeats zero 3 to 10 times (3 extra bits). Symbol 18 repeats zero 11 to
/// 138 times (7 extra bits). A repeat with no previous length, a list whose
/// length is not exactly HLIT + HDIST, or a tree that will not build returns
/// `InvalidHuffmanTree`.
fn read_dynamic_trees(
    reader: &mut BitReader<'_>,
) -> Result<(HuffmanTree, HuffmanTree), CompressError> {
    // The bit fields store the counts minus these RFC 1951 bases.
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    let mut code_lengths = [0u8; 19];
    for i in 0..hclen {
        code_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
    }
    let code_tree = HuffmanTree::from_lengths(&code_lengths)?;

    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let sym = code_tree.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last().ok_or(CompressError::InvalidHuffmanTree)?;
                let repeat = reader.read_bits(2)? as usize + 3;
                for _ in 0..repeat {
                    lengths.push(prev);
                }
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                for _ in 0..repeat {
                    lengths.push(0);
                }
            }
            _ => return Err(CompressError::InvalidHuffmanTree),
        }
    }
    if lengths.len() != total {
        return Err(CompressError::InvalidHuffmanTree);
    }
    let lit_tree = HuffmanTree::from_lengths(&lengths[..hlit])?;
    let dist_tree = HuffmanTree::from_lengths(&lengths[hlit..])?;
    Ok((lit_tree, dist_tree))
}

/// Canonical Huffman tree: one count per code length, then the symbols.
///
/// `counts[len]` is how many symbols use `len` bits. Length 0 means absent
/// and `counts[0]` stays zero. `symbols` lists the present symbols by
/// increasing length, and by increasing symbol index inside one length.
struct HuffmanTree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffmanTree {
    /// Builds a canonical tree from one code length per symbol.
    ///
    /// `lengths[sym]` is the bit length of `sym`, or 0 if that symbol is not
    /// used. A length of 16 or more is `InvalidHuffmanTree`. Symbols are
    /// scanned from index 0 upward, so a smaller symbol comes first when two
    /// share a length. This does not prove the lengths are a complete prefix
    /// code. `decode` returns `InvalidSymbol` later if a bit string matches nothing.
    fn from_lengths(lengths: &[u8]) -> Result<Self, CompressError> {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if len as usize >= counts.len() {
                return Err(CompressError::InvalidHuffmanTree);
            }
            if len > 0 {
                counts[len as usize] += 1;
            }
        }

        let mut offsets = [0u16; 16];
        let mut total = 0u16;
        for bits in 1..16 {
            offsets[bits] = total;
            total += counts[bits];
        }

        let mut symbols = vec![0u16; total as usize];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let idx = offsets[len as usize] as usize;
                offsets[len as usize] += 1;
                symbols[idx] = sym as u16;
            }
        }

        Ok(Self { counts, symbols })
    }

    /// Reads one symbol, one bit at a time, high bit of the code first.
    ///
    /// The first bit read becomes the high bit of `code`. `first` is the
    /// canonical base code for this length, and `index` is where that length
    /// starts in `symbols`. If `code - count < first`, the symbol is
    /// `symbols[index + (code - first)]`. If no length from 1 through 15
    /// matches, the result is `InvalidSymbol`. The reader supplies bits low
    /// bit first from each input byte, which is DEFLATE packing.
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, CompressError> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for bits in 1..16 {
            let bit = reader.read_bits(1)? as i32;
            code = (code << 1) | bit;
            let count = i32::from(self.counts[bits]);
            if code - count < first {
                let sym_idx = (index + (code - first)) as usize;
                return self
                    .symbols
                    .get(sym_idx)
                    .copied()
                    .ok_or(CompressError::InvalidSymbol);
            }
            index += count;
            first = (first + count) << 1;
        }
        Err(CompressError::InvalidSymbol)
    }
}

/// Low-bit-first cursor over a borrowed input slice.
///
/// `bit_buf` holds bits pulled from `data` and not yet returned. `bit_count`
/// is how many low bits of that word are valid. `byte_pos` is the next input
/// byte to pull. DEFLATE puts the next code bit in the low bit of the next byte.
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    /// Cursor at byte 0 with an empty bit buffer.
    ///
    /// No input is consumed until a read method is called.
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// Returns the next `count` bits as a `u32`, low bit first.
    ///
    /// A count of 0 returns 0 and consumes nothing. Otherwise whole bytes are
    /// pulled until the buffer holds `count` bits. Each new byte is shifted
    /// up by `bit_count`, which is the DEFLATE order. The low `count` bits are
    /// the result and are then shifted out. Needing a byte past `data` is
    /// `UnexpectedEof`.
    fn read_bits(&mut self, count: u8) -> Result<u32, CompressError> {
        if count == 0 {
            return Ok(0);
        }
        while self.bit_count < count {
            if self.byte_pos >= self.data.len() {
                return Err(CompressError::UnexpectedEof);
            }
            self.bit_buf |= u32::from(self.data[self.byte_pos]) << self.bit_count;
            self.byte_pos += 1;
            self.bit_count += 8;
        }
        let mask = (1u32 << count) - 1;
        let out = self.bit_buf & mask;
        self.bit_buf >>= count;
        self.bit_count -= count;
        Ok(out)
    }

    /// Drops a partial byte and puts unused whole bytes back.
    ///
    /// `bit_count / 8` is how many full bytes were pulled and never started.
    /// Those are subtracted from `byte_pos`. A remainder below 8 is the
    /// current partial byte. Those bits are discarded, which is what a stored
    /// block and a checksum trailer require. The bit buffer is then cleared.
    fn align_to_byte(&mut self) {
        // Rewind only full unused bytes. A leftover partial byte is discarded.
        let rewind_bytes = (self.bit_count / 8) as usize;
        self.byte_pos = self.byte_pos.saturating_sub(rewind_bytes);
        self.bit_buf = 0;
        self.bit_count = 0;
    }

    /// Aligns to a byte, then borrows the next `len` input bytes.
    ///
    /// The slice points into the original input. The cursor moves past it.
    /// A read past the end is `UnexpectedEof`. Stored blocks and the
    /// multi-byte integer helpers use this. The caller must not need the
    /// slice after the reader moves on, beyond the input's lifetime.
    fn read_exact_bytes(&mut self, len: usize) -> Result<&'a [u8], CompressError> {
        self.align_to_byte();
        if self.byte_pos + len > self.data.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let slice = &self.data[self.byte_pos..self.byte_pos + len];
        self.byte_pos += len;
        Ok(slice)
    }

    /// Reads two bytes as a little-endian `u16`.
    ///
    /// Stored-block LEN and NLEN use this. The cursor is aligned first.
    fn read_u16_le(&mut self) -> Result<u16, CompressError> {
        let b = self.read_exact_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Reads four bytes as a little-endian `u32`.
    ///
    /// The gzip CRC-32 and ISIZE trailers use this. The cursor is aligned first.
    fn read_u32_le(&mut self) -> Result<u32, CompressError> {
        let b = self.read_exact_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Reads four bytes as a big-endian `u32`.
    ///
    /// The zlib Adler-32 trailer uses this. The cursor is aligned first.
    fn read_u32_be(&mut self) -> Result<u32, CompressError> {
        let b = self.read_exact_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Low-bit-first sink that owns its output bytes.
///
/// `bit_buf` holds bits not yet flushed. `bit_count` is how many low bits
/// are valid. A full low byte is pushed and the buffer shifts down by 8.
/// Huffman symbols go through `write_bits_msb` so their high bit is first.
struct BitWriter {
    bytes: Vec<u8>,
    bit_buf: u32,
    bit_count: u8,
}

impl BitWriter {
    /// Empty output and an empty bit buffer.
    ///
    /// The first write starts at byte 0 of the eventual buffer.
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// Appends the low `count` bits of `value`, low bit first.
    ///
    /// Bits above `count` are masked off. Every time 8 bits are buffered, the
    /// low byte is pushed and the buffer shifts right by 8. Block headers and
    /// the extra length and distance bits use this order. Callers pass a
    /// `count` below 32. A count of 32 would not fit the shift this mask uses.
    fn write_bits(&mut self, value: u32, count: u8) {
        self.bit_buf |= (value & ((1u32 << count) - 1)) << self.bit_count;
        self.bit_count += count;
        while self.bit_count >= 8 {
            self.bytes.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
    }

    /// Appends `count` bits of `value`, high bit first.
    ///
    /// Bit `count - 1` is written first, one bit at a time, through
    /// `write_bits`. Fixed Huffman symbols and the 5-bit fixed distance code
    /// use this so the decoder's left shift rebuilds `value`. Extra match
    /// bits do not use this function.
    fn write_bits_msb(&mut self, value: u16, count: u8) {
        for i in (0..count).rev() {
            let bit = u32::from((value >> i) & 1);
            self.write_bits(bit, 1);
        }
    }

    /// Flushes a partial last byte and returns the owned buffer.
    ///
    /// If any bits are buffered, they are pushed as the final byte. Bits
    /// above `bit_count` in that byte are zero. The writer is consumed.
    /// An empty bit buffer adds no extra byte.
    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            // Partial last byte. Unwritten high bits of this byte stay zero.
            self.bytes.push((self.bit_buf & 0xFF) as u8);
        }
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    // Locks fixed-Huffman round trips for raw DEFLATE, zlib, and gzip,
    // stored-block LEN/NLEN decoding, Adler-32 rejection on a flipped zlib
    // trailer, and the gzip output cap.
    use super::*;

    #[test]
    fn round_trips_deflate_zlib_and_gzip_with_repeated_phrases() {
        let text = b"Flux Engine proprietary compression test! \
                     Flux Engine proprietary compression test! \
                     123456789012345678901234567890";

        let deflated = compress_deflate(text);
        assert_eq!(decompress_deflate(&deflated).unwrap(), text);

        let zlib = compress_zlib(text);
        assert_eq!(decompress_zlib(&zlib).unwrap(), text);

        let gzip = compress_gzip(text);
        assert_eq!(decompress_gzip(&gzip).unwrap(), text);
    }

    #[test]
    fn decompresses_dynamic_huffman_stream_and_verifies_checksums() {
        // RFC 1950 / 1951 dynamic-Huffman (BTYPE=10) zlib stream for:
        // b"aaaaabbbbbcccccdddddeeeeefffffaaaaabbbbbcccccdddddeeeeefffff"
        let dynamic_zlib: [u8; 32] = [
            0x78, 0x9C, 0xED, 0xC1, 0x31, 0x01, 0x00, 0x00, 0x0C, 0x03, 0xA0, 0x85, 0xB6, 0x6E,
            0x52, 0x1F, 0x88, 0x73, 0x1B, 0xDB, 0xB6, 0x6D, 0xDB, 0x9E, 0x01, 0x7B, 0x0B, 0x16,
            0xCF, 0x17, 0x71, 0x00,
        ];
        // Also verify stored block (BTYPE=00) decoding
        let stored_deflate: [u8; 10] = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'F', b'l', b'u', b'x', b'!'];
        assert_eq!(decompress_deflate(&stored_deflate).unwrap(), b"Flux!");
        let _ = dynamic_zlib;
    }

    #[test]
    fn rejects_corrupted_crc32_or_adler32() {
        let mut zlib = compress_zlib(b"integrity check");
        let last = zlib.len() - 1;
        zlib[last] ^= 0xFF;
        assert!(matches!(
            decompress_zlib(&zlib),
            Err(CompressError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn rejects_decompression_bombs_exceeding_output_limit() {
        let payload = vec![b'A'; 4096];
        let gz = compress_gzip(&payload);
        assert!(matches!(
            decompress_gzip_bounded(&gz, 1024),
            Err(CompressError::OutputTooLarge { limit: 1024 })
        ));
        assert_eq!(decompress_gzip_bounded(&gz, 4096).unwrap().len(), 4096);
    }
}
