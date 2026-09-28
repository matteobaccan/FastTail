## ADDED Requirements

### Requirement: Action Table
Every keyboard action of the window, of the focused stream and of the result lists SHALL be defined once in an action table, with a stable identifier (such as `search.focus`, `search.next`, `bookmark.toggle`, `view.follow`, `tab.activate.3`), a localized name, a scope (window, stream or list) and its default bindings, and the default bindings SHALL equal the shortcuts of FastTail 0.12.0. Every handler SHALL dispatch through the table, and every menu label, tooltip and the help dialog that shows a shortcut SHALL show the current binding from the table, in the platform's notation. The keys `Esc`, `Enter`, `Tab`, `SHIFT + Tab`, and inside a text field the arrows, `Home`, `End`, `PgUp`, `PgDown` and `CTRL + A / C / V / X / Z`, SHALL be fixed and not bindable. A stream-scope binding without a modifier SHALL be ignored while a text field has the keyboard.

#### Scenario: Defaults unchanged
- **WHEN** `fasttail.ini` has no `[shortcuts]` section
- **THEN** every shortcut listed in the Keyboard Navigation & Hotkeys requirement acts as in FastTail 0.12.0.

#### Scenario: Labels follow the binding
- **WHEN** the user rebinds "Show in context" to `Alt+C`
- **THEN** the row context menu shows "Show in context  (ALT + C)" and the help dialog lists `ALT + C` for it.

### Requirement: Rebinding in Settings
Settings SHALL offer a "Keyboard shortcuts" page listing every action of the table grouped by scope, and the external tools that have a shortcut, with a search field over names and bindings. The user SHALL be able to record a new binding by pressing it (the pressed keys SHALL NOT reach the application while recording and `Esc` SHALL cancel), add a second binding, remove a binding, reset one action and reset all actions to their defaults. Removing the last binding of a stream navigation action (scrolling by line, page, to the top or the end) SHALL be refused. A change SHALL take effect immediately, without restart.

#### Scenario: Rebinding a bookmark key
- **WHEN** the user records `Ctrl+B` for "Toggle bookmark"
- **THEN** `CTRL + B` toggles the bookmark of the current row, `CTRL + F2` no longer does, and `[shortcuts]` holds `bookmark.toggle=Ctrl+B`.

#### Scenario: Reset all
- **WHEN** the user has changed five bindings and presses "Reset all"
- **THEN** every action has its default binding again and `[shortcuts]` is empty.

#### Scenario: Recording a fixed key
- **WHEN** the user presses `Tab` while recording
- **THEN** the recorder says the key is reserved and the binding is not changed.

### Requirement: Shortcut Conflicts
A binding SHALL conflict with another action's binding when both are in the same scope or one of them is in the window scope, and with an external tool's shortcut in every case. When the user records a conflicting binding, the recorder SHALL name the action or tool holding it and offer to move the binding (the holder loses it) or cancel. When `fasttail.ini` holds conflicting bindings, the action earlier in the table SHALL keep the binding, the later one SHALL lose it, and Settings SHALL list the conflicts once; a tool shortcut conflicting with a built-in action SHALL be flagged in the tool's row and SHALL NOT fire. Stream-scope and list-scope bindings SHALL NOT conflict with each other.

#### Scenario: Moving a binding
- **WHEN** the user records `F3` for "Go to line" and `F3` is bound to "Next match"
- **THEN** the recorder says `F3` is used by "Next match"; choosing "Move here" binds `F3` to "Go to line" and leaves "Next match" with its other bindings only.

#### Scenario: Tool shadowing a built-in
- **WHEN** an external tool has the shortcut `Ctrl+Shift+F`
- **THEN** its row in Settings is flagged as conflicting with "Find in all streams", and `CTRL + SHIFT + F` opens the Find results tab.

### Requirement: Shortcut Persistence
Bindings that differ from the defaults SHALL be saved in `fasttail.ini` section `[shortcuts]`, one key per action identifier, whose value is a comma-separated list of bindings in canonical form (`Ctrl+Shift+F9`, `Alt+W`, `F3`, `Space`) or an empty value for an action left unbound. `Cmd` and `Ctrl` SHALL both be read as the platform's command modifier. Unknown identifiers and values that do not parse SHALL be ignored and reported in Settings. External tool shortcuts SHALL stay in their `[tool.N]` sections.

#### Scenario: Two bindings
- **WHEN** `[shortcuts]` holds `search.next=F3, Ctrl+N`
- **THEN** both `F3` and `CTRL + N` go to the next match of the focused stream.

#### Scenario: Bad value
- **WHEN** `[shortcuts]` holds `search.next=Ctrl+Banana`
- **THEN** "Next match" keeps its default binding and Settings reports the ignored value.
