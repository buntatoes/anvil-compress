# Security

Report problems in private:

https://github.com/buntatoes/anvil-compress/security/advisories/new

The browser repository takes the same kind of report:

https://github.com/buntatoes/anvil-browser/security/advisories/new

Do not put the details in a public issue.

## What is checked

- Decompress stops at the output cap. The default is 32 MiB.
- A zlib stream with a bad Adler-32 is rejected.
- A gzip stream with a bad CRC-32 or ISIZE trailer is rejected.

Version 0.3.1 is current.
