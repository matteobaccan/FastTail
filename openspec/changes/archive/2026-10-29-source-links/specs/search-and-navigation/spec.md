## ADDED Requirements

### Requirement: Source Locations in Lines
The application SHALL recognise source locations in the rows it shows (`path:line[:column]`, Java and Kotlin frames, Python, .NET, Go, Node and Rust shapes) without matching times of day. Ctrl + click on a location, or the row menu, SHALL open it with the editor command of Settings, whose `{file}`, `{line}` and `{col}` placeholders are each expanded inside one argument and run without a shell.

#### Scenario: A Java stack frame
- **WHEN** a row holds `at com.acme.Orders.save(Orders.java:42)`, the source roots include `D:\src\shop\src\main\java` and the editor command is `code -g {file}:{line}`
- **THEN** Ctrl + click on the frame runs `code` with `D:\src\shop\src\main\java\com\acme\Orders.java:42` as one argument.

#### Scenario: A time is not a location
- **WHEN** a row starts with `12:30:45 INFO started`
- **THEN** nothing in it is recognised as a source location.

### Requirement: Source Roots
A relative location SHALL be resolved against the source roots in order, the first existing file winning; a location whose file is not found SHALL say so and run nothing.

#### Scenario: File not found
- **WHEN** the user Ctrl + clicks `src/missing.rs:3` and no source root holds it
- **THEN** a notice says the file was not found under the source roots and no program runs.
