## ADDED Requirements

### Requirement: Standard Input Stream
Standard input SHALL be copied by a background thread into a temporary spool file, flushed as it arrives, and the spool SHALL be tailed as a regular followed stream, so that every stream feature works on it and no copy of the input is held in memory. The stream SHALL be titled `stdin`, SHALL identify itself in the footer as standard input, and SHALL NOT be saved in the workspace, the recent files or session files. When the input ends, the stream SHALL stay open and say that the input ended. The spool SHALL be bounded by `stdin_spool_max_mb` (default 2048 MB, configurable in `fasttail.ini` and Settings) and by a 512 MB free-space margin on its volume: on reaching either limit the spool SHALL restart from empty, which the stream handles as a truncation, and the stream bar SHALL say that earlier input was discarded. The spool file SHALL be deleted when the stream is closed and at exit, and a spool left by a crashed process SHALL be deleted at the next start. On Windows this SHALL work with the GUI-subsystem executable, reading the standard-input handle passed by the shell.

#### Scenario: Following a container
- **WHEN** `docker logs -f api | fasttail -` runs and the container writes a line every second
- **THEN** each line appears in the `stdin` stream within 50 milliseconds of arriving, and search and highlight rules apply to it.

#### Scenario: Producer finishes
- **WHEN** `cat build.log | fasttail -` has copied the whole file and the pipe closes
- **THEN** the stream keeps all lines and its stream bar says the input ended.

#### Scenario: Spool limit reached
- **WHEN** a producer has written 2048 MB into the stdin stream with the default limit
- **THEN** the spool restarts from empty, new lines keep arriving, and the stream bar says earlier input was discarded because of the limit.

#### Scenario: Not restored after restart
- **WHEN** FastTail is closed with a `stdin` stream open and started again without piped input
- **THEN** no `stdin` stream is restored and no spool file remains on disk.
