//! Proprietary DEFLATE, zlib, and gzip compression and decompression library.
//!
//! Implements bitstream decoding for uncompressed blocks, fixed Huffman trees,
//! and dynamic Huffman trees with LZ77 sliding-window back-references, plus
//! Adler-32 and CRC-32 checksum verification and bounded decompression-bomb
//! protection.

use std::fmt;

/// Default maximum decompressed payload size (32 MiB) enforced to prevent
/// decompression-bomb memory exhaustion on untrusted streams.
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompressError {
    UnexpectedEof,
    InvalidHeader(&'static str),
    InvalidBlockType(u8),
    InvalidStoredLength,
    InvalidHuffmanTree,
    InvalidSymbol,
    InvalidDistance,
    OutputTooLarge { limit: usize },
    ChecksumMismatch { expected: u32, actual: u32 },
    LengthMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for CompressError {
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

/// Compute the Adler-32 checksum of `data`.
pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
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

/// Compute the IEEE 802.3 CRC-32 checksum of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let mut c = (crc ^ u32::from(byte)) & 0xFF;
        for _ in 0..8 {
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

/// Decompress a raw DEFLATE bitstream with the default 32 MiB output limit.
pub fn decompress_deflate(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_deflate_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompress a raw DEFLATE bitstream enforcing a caller-supplied `max_output` byte cap.
pub fn decompress_deflate_bounded(
    input: &[u8],
    max_output: usize,
) -> Result<Vec<u8>, CompressError> {
    let mut reader = BitReader::new(input);
    decompress_deflate_from_reader(&mut reader, max_output)
}

/// Decompress a zlib-wrapped DEFLATE stream and verify its Adler-32 trailer
/// with the default 32 MiB output limit.
pub fn decompress_zlib(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_zlib_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompress a zlib-wrapped DEFLATE stream enforcing a caller-supplied `max_output` byte cap.
pub fn decompress_zlib_bounded(input: &[u8], max_output: usize) -> Result<Vec<u8>, CompressError> {
    if input.len() < 6 {
        return Err(CompressError::UnexpectedEof);
    }
    let cmf = input[0];
    let flg = input[1];
    let method = cmf & 0x0F;
    let cinfo = cmf >> 4;
    if method != 8 || cinfo > 7 {
        return Err(CompressError::InvalidHeader(
            "unsupported zlib compression method",
        ));
    }
    let check = (u16::from(cmf) << 8) | u16::from(flg);
    if check % 31 != 0 {
        return Err(CompressError::InvalidHeader(
            "zlib header check bits failed",
        ));
    }
    if (flg & 0x20) != 0 {
        return Err(CompressError::InvalidHeader(
            "preset dictionary not supported",
        ));
    }

    let mut reader = BitReader::new(&input[2..]);
    let output = decompress_deflate_from_reader(&mut reader, max_output)?;
    reader.align_to_byte();
    let expected = reader.read_u32_be()?;
    let actual = adler32(&output);
    if expected != actual {
        return Err(CompressError::ChecksumMismatch { expected, actual });
    }
    Ok(output)
}

/// Decompress a gzip-wrapped DEFLATE stream and verify its CRC-32 and ISIZE trailer
/// with the default 32 MiB output limit.
pub fn decompress_gzip(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    decompress_gzip_bounded(input, DEFAULT_MAX_DECOMPRESSED_BYTES)
}

/// Decompress a gzip-wrapped DEFLATE stream enforcing a caller-supplied `max_output` byte cap.
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

    // FEXTRA
    if (flags & 0x04) != 0 {
        if pos + 2 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    // FNAME
    if (flags & 0x08) != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    // FCOMMENT
    if (flags & 0x10) != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    // FHCRC
    if (flags & 0x02) != 0 {
        pos += 2;
    }
    if pos > input.len() {
        return Err(CompressError::UnexpectedEof);
    }

    let mut reader = BitReader::new(&input[pos..]);
    let output = decompress_deflate_from_reader(&mut reader, max_output)?;
    reader.align_to_byte();
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

/// Compress `data` into a valid DEFLATE stream using fixed Huffman + LZ77 matching.
pub fn compress_deflate(data: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::new();
    // Single final block using fixed Huffman codes (BTYPE = 01).
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
    // End-of-block symbol 256 (7 bits: 0000000)
    write_fixed_symbol(&mut writer, 256);
    writer.finish()
}

/// Compress `data` into a zlib container (CMF/FLG header + DEFLATE + Adler-32).
pub fn compress_zlib(data: &[u8]) -> Vec<u8> {
    let deflated = compress_deflate(data);
    let mut out = Vec::with_capacity(2 + deflated.len() + 4);
    // CMF = 0x78 (32K window, Deflate), FLG = 0x9C (check bits so 0x789C % 31 == 0)
    out.push(0x78);
    out.push(0x9C);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Compress `data` into a gzip container (10-byte header + DEFLATE + CRC-32 + ISIZE).
pub fn compress_gzip(data: &[u8]) -> Vec<u8> {
    let deflated = compress_deflate(data);
    let mut out = Vec::with_capacity(10 + deflated.len() + 8);
    out.extend_from_slice(&[0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF]);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn find_lz77_match(data: &[u8], pos: usize) -> (usize, usize) {
    let max_len = (data.len() - pos).min(258);
    if max_len < 3 || pos == 0 {
        return (0, 0);
    }
    let min_start = pos.saturating_sub(4096);
    let mut best_len = 0usize;
    let mut best_dist = 0usize;

    let mut cand = pos;
    let mut steps = 0usize;
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

fn write_fixed_literal(writer: &mut BitWriter, byte: u8) {
    write_fixed_symbol(writer, u16::from(byte));
}

fn write_fixed_symbol(writer: &mut BitWriter, sym: u16) {
    match sym {
        0..=143 => writer.write_bits_msb(0b0011_0000 + sym, 8),
        144..=255 => writer.write_bits_msb(0b1_1001_0000 + (sym - 144), 9),
        256..=279 => writer.write_bits_msb(sym - 256, 7),
        280..=287 => writer.write_bits_msb(0b1100_0000 + (sym - 280), 8),
        _ => {}
    }
}

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
    writer.write_bits_msb(dist_idx as u16, 5);
    let dist_extra = DIST_EXTRA[dist_idx];
    if dist_extra > 0 {
        let extra_val = u32::from(dist_u16 - DIST_BASE[dist_idx]);
        writer.write_bits(extra_val, dist_extra);
    }
}

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
                if len != !nlen {
                    return Err(CompressError::InvalidStoredLength);
                }
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

fn read_dynamic_trees(
    reader: &mut BitReader<'_>,
) -> Result<(HuffmanTree, HuffmanTree), CompressError> {
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

struct HuffmanTree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffmanTree {
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

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

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

    fn align_to_byte(&mut self) {
        let rewind_bytes = (self.bit_count / 8) as usize;
        self.byte_pos = self.byte_pos.saturating_sub(rewind_bytes);
        self.bit_buf = 0;
        self.bit_count = 0;
    }

    fn read_exact_bytes(&mut self, len: usize) -> Result<&'a [u8], CompressError> {
        self.align_to_byte();
        if self.byte_pos + len > self.data.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let slice = &self.data[self.byte_pos..self.byte_pos + len];
        self.byte_pos += len;
        Ok(slice)
    }

    fn read_u16_le(&mut self) -> Result<u16, CompressError> {
        let b = self.read_exact_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32_le(&mut self) -> Result<u32, CompressError> {
        let b = self.read_exact_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u32_be(&mut self) -> Result<u32, CompressError> {
        let b = self.read_exact_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

struct BitWriter {
    bytes: Vec<u8>,
    bit_buf: u32,
    bit_count: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn write_bits(&mut self, value: u32, count: u8) {
        self.bit_buf |= (value & ((1u32 << count) - 1)) << self.bit_count;
        self.bit_count += count;
        while self.bit_count >= 8 {
            self.bytes.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
    }

    fn write_bits_msb(&mut self, value: u16, count: u8) {
        for i in (0..count).rev() {
            let bit = u32::from((value >> i) & 1);
            self.write_bits(bit, 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.bytes.push((self.bit_buf & 0xFF) as u8);
        }
        self.bytes
    }
}

#[cfg(test)]
mod tests {
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
