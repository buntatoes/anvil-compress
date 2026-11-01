# Security

Report problems in private:

https://github.com/buntatoes/anvil-compress/security/advisories/new

Do not put the details in a public issue.

## What is checked

- Decompression stops at the output cap. The default is 32 MiB (`DefaultMaxDecompressedBytes`).
- A zlib stream with a bad Adler-32 checksum or invalid CMF/FLG header is rejected.
- A gzip stream (including each member of a concatenated multi-member stream) with a bad CRC-32 or ISIZE trailer is rejected.

Version 0.4.0 (`v.04`) is current.
