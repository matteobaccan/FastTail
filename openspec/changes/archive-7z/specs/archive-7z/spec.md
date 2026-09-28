## ADDED Requirements

### Requirement: 7z Archive Input
The engine SHALL recognise a 7z archive by its signature `37 7A BC AF 27 1C` at the start of the file, whatever its extension, and SHALL open it like a zip archive under the rules of Compressed Log Input, Decompression Space Guard and Temporary Spool Lifecycle: an archive with one file entry that can be opened SHALL open that entry directly, an archive with several SHALL show the entry picker listing each file entry with its name, its size and, when it cannot be opened, the reason, and each chosen entry SHALL open as its own stream titled `<archive name> › <entry>` with the identity `<archive>/<entry>`. Entries using the copy, LZMA, LZMA2, BZip2, Deflate or PPMd coders, with or without the BCJ, BCJ2, ARM or delta filters, SHALL be extracted, including entries of solid archives, whose block SHALL be decoded from its start while the entries before the chosen one are skipped without being written, the progress being shown over the block. An entry whose first bytes are gzip, bzip2, xz or zstd SHALL be decompressed once more; an archive inside an entry SHALL NOT be unpacked.

#### Scenario: Support bundle in 7z
- **WHEN** the user opens `logs.7z` holding `server.log` and `worker.log`
- **THEN** the entry picker lists both with their sizes, and choosing both opens two streams titled `logs.7z › server.log` and `logs.7z › worker.log`.

#### Scenario: Last entry of a solid archive
- **WHEN** the user opens the last of 50 entries of a solid 7z block of 2 GB
- **THEN** the stream bar shows the extraction progress rising over the block, only that entry is written to the spool, and its lines appear once the decoder reaches it.

#### Scenario: Session round trip
- **WHEN** a session holding the stream `logs.7z › server.log` with 2 bookmarks is loaded
- **THEN** the entry is extracted again in the background without showing the picker, and both bookmarks appear once their lines are indexed.

### Requirement: 7z Refusals and Limits
The picker SHALL show disabled, with the reason, an entry that is encrypted, uses another coder, has an absolute, drive-prefixed or climbing (`../`) name, or has a stream path taken by an earlier entry; an archive whose header is encrypted SHALL be refused as a whole with a message, and no password SHALL be asked. An entry whose LZMA, LZMA2 or PPMd dictionary exceeds 256 MiB SHALL be refused before that memory is allocated. The header SHALL be decoded with at most 64 MB of memory and at most 100 000 entries SHALL be listed; beyond either bound the picker SHALL say that the list is partial. No file SHALL be created at a path taken from the archive.

#### Scenario: Encrypted entry
- **WHEN** a 7z archive holds `secret.log` encrypted with AES and `app.log` unencrypted
- **THEN** `secret.log` is shown disabled as encrypted and `app.log` opens.

#### Scenario: Oversized dictionary
- **WHEN** an entry was compressed with a 1536 MiB LZMA2 dictionary
- **THEN** it is refused with a message naming the 256 MiB limit and no spool data is written for it.

#### Scenario: Unsafe name
- **WHEN** a 7z archive holds an entry named `../../evil.log`
- **THEN** the entry is shown disabled with the unsafe-name reason and nothing is written outside the spool.
