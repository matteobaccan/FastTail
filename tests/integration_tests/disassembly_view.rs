// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The ASM view (openspec/changes/disassembly-view) through the engine: rows of known
//! code, scrolling both ways, search, follow and truncation.

use fasttail::tail_engine::{TailEngine, ViewMode};
use std::time::Duration;

/// push rbp; mov rbp,rsp; sub rsp,10h; mov eax,0; leave; ret, then 5 bytes of data
/// that are not code (`0F 0B` is `ud2`, `FF FF` is invalid), `n` times.
fn code(n: usize) -> Vec<u8> {
    let function = [
        0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC, 0x10, 0xB8, 0x00, 0x00, 0x00, 0x00, 0xC9, 0xC3,
        0xFF, 0xFF, 0x0F, 0x0B, 0xCC,
    ];
    function.repeat(n)
}

fn open(bytes: &[u8]) -> (TailEngine, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("code.bin");
    std::fs::write(&path, bytes).unwrap();
    let mut engine = TailEngine::open(&path).unwrap();
    engine.set_view_mode(ViewMode::Asm);
    (engine, dir, path)
}

#[test]
fn the_rows_of_known_code_and_scrolling_down_then_up() {
    let (mut engine, _dir, _path) = open(&code(50));
    assert_eq!(engine.asm.top, 0, "a raw file starts at offset 0");
    let rows = engine.asm_rows(10);
    let text: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(
        &text[..6],
        [
            "push rbp",
            "mov rbp,rsp",
            "sub rsp,10h",
            "mov eax,0",
            "leave",
            "ret"
        ]
    );
    assert!(rows[6].data, "0xFF is not an instruction: {:?}", rows[6]);
    // Down 30 rows, up 30 rows: the same offsets on the way back.
    let mut offsets = vec![engine.asm.top];
    for _ in 0..30 {
        engine.asm_scroll(1);
        offsets.push(engine.asm.top);
    }
    for expected in offsets.iter().rev().skip(1) {
        engine.asm_scroll(-1);
        assert_eq!(engine.asm.top, *expected);
    }
}

#[test]
fn a_search_hit_and_go_to_start_a_row() {
    let (mut engine, _dir, _path) = open(&code(50));
    engine.search_query = "C9 C3".into();
    engine.update_search("C9 C3");
    let target = engine.search_next(false).expect("a byte hit");
    assert_eq!(target % 20, 13, "leave; ret of a function");
    assert!(engine.asm_go_to(&target.to_string()));
    assert_eq!(engine.asm_rows(1)[0].text, "leave");
    assert!(engine.asm_go_to("0x14"));
    assert_eq!(engine.asm_rows(1)[0].text, "push rbp");
    assert!(!engine.asm_go_to("entry"), "no entry point in a raw file");
}

#[test]
fn follow_shows_the_end_and_truncation_goes_back_to_the_start() {
    let (mut engine, _dir, path) = open(&code(50));
    engine.asm_bottom(5);
    let rows = engine.asm_rows(5);
    let last = rows.last().unwrap();
    assert_eq!(last.offset + last.bytes.len() as u64, engine.file_size);
    // The data bytes decode as they do on a CPU: FF 0F is `dec`, then 0B CC.
    assert_eq!(last.text, "or ecx,esp");

    std::fs::write(&path, code(1)).unwrap();
    engine.size_check_interval = Duration::ZERO;
    for _ in 0..20 {
        engine.poll_updates();
        if engine.file_size == 20 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(engine.file_size, 20);
    let rows = engine.asm_rows(5);
    assert_eq!(engine.asm.top, 0);
    assert_eq!(rows[0].text, "push rbp");
}
