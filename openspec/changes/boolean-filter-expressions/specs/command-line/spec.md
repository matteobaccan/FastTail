## ADDED Requirements

### Requirement: Filter Expression Option
The executable SHALL accept `--filter-expr <expr>`, which SHALL put the streams opened from the command line, the standard-input stream included, in filter expression mode with that expression (see the filters-and-highlighting capability). `--filter-expr` together with `--filter` SHALL be a usage error. An expression that does not parse SHALL be reported on stderr with its position before any window opens, and the process SHALL exit with code 2.

#### Scenario: OR from a shell
- **WHEN** the user runs `fasttail --fresh --filter-expr "payment OR refund" app.log`
- **THEN** `app.log` opens in expression mode and shows the lines containing `payment` or `refund`.

#### Scenario: Invalid expression
- **WHEN** the user runs `fasttail --filter-expr "(payment OR" app.log`
- **THEN** stderr reports the syntax error and its position, no window opens, and the exit code is 2.

#### Scenario: Conflicting options
- **WHEN** the user runs `fasttail --filter ERROR --filter-expr "a OR b" app.log`
- **THEN** usage is printed to stderr and the process exits with code 2.
