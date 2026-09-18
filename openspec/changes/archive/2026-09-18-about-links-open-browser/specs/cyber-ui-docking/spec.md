## MODIFIED Requirements

### Requirement: Clickable External Hyperlinks in About Dialog
The About dialog SHALL provide clickable hyperlinks to the GitHub repository and to `https://www.baccan.it`, each showing its full URL as a hover tooltip, and SHALL show the git tag and build timestamp of the running binary. The build SHALL include the platform support that opens URLs in the operating system's default browser (the `links` feature of eframe), and a test SHALL fail if that support is dropped from the dependency declaration.

#### Scenario: User clicks website or repository link
- **WHEN** the user opens the About dialog and clicks the GitHub repository or `www.baccan.it` link
- **THEN** the default web browser opens the respective URL.

#### Scenario: Hovering a link
- **WHEN** the pointer rests on the `www.baccan.it` or the repository link
- **THEN** a tooltip shows the full `https://` URL that a click will open.

#### Scenario: Browser support removed from the build
- **WHEN** the `eframe` dependency in `Cargo.toml` no longer lists the `links` feature
- **THEN** the test suite fails with a message naming the missing feature.
