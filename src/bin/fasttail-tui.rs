// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! `fasttail-tui`: the terminal interface (see `fasttail::tui`).

fn main() {
    std::process::exit(fasttail::tui::run(std::env::args().skip(1)));
}
