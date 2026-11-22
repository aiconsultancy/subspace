use super::super::framing::read_frame;
use crate::project::limits;
use std::io::{BufReader, Cursor};

#[test]
fn newline_frame_accepts_the_exact_budget_and_rejects_one_byte_more() {
    let mut bytes = vec![b'x'; limits::FRAME_BYTES - 1];
    bytes.push(b'\n');
    let frame = read_frame(&mut BufReader::new(Cursor::new(bytes.clone())))
        .expect("exact budget frame")
        .expect("one frame");
    assert_eq!(frame.len(), limits::FRAME_BYTES - 1);
    bytes.insert(0, b'x');
    assert!(read_frame(&mut BufReader::new(Cursor::new(bytes))).is_err());
}

#[test]
fn eof_is_distinct_from_an_unterminated_or_oversized_worker_frame() {
    assert!(
        read_frame(&mut BufReader::new(Cursor::new(Vec::<u8>::new())))
            .expect("clean EOF")
            .is_none()
    );
    assert!(read_frame(&mut BufReader::new(Cursor::new(b"unterminated"))).is_err());
    let bytes = vec![b'x'; limits::FRAME_BYTES + 1];
    assert!(read_frame(&mut BufReader::new(Cursor::new(bytes))).is_err());
}
