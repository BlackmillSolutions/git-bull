//! Reading Git output record by record while it streams in.

use std::io::{self, BufRead, BufReader, Read};

/// Splits a stream into records ended by `delimiter`, usually NUL for `-z`
/// output or a newline.
///
/// Each record is returned without its delimiter. A last record without a
/// delimiter is returned as well; empty records are kept.
pub struct Records<R> {
    reader: BufReader<R>,
    delimiter: u8,
    record: Vec<u8>,
}

impl<R: Read> Records<R> {
    pub fn new(reader: R, delimiter: u8) -> Records<R> {
        Records {
            reader: BufReader::with_capacity(64 * 1024, reader),
            delimiter,
            record: Vec::new(),
        }
    }

    /// The next record, or `None` at the end of the stream.
    pub fn next_record(&mut self) -> io::Result<Option<&[u8]>> {
        self.record.clear();
        let read = self.reader.read_until(self.delimiter, &mut self.record)?;
        if read == 0 {
            return Ok(None);
        }
        if self.record.last() == Some(&self.delimiter) {
            self.record.pop();
        }
        Ok(Some(&self.record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hands out its data in pieces of at most `chunk` bytes, as a pipe does.
    struct Chunked {
        data: Vec<u8>,
        position: usize,
        chunk: usize,
    }

    impl Read for Chunked {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let end = (self.position + self.chunk.min(buf.len())).min(self.data.len());
            let piece = &self.data[self.position..end];
            buf[..piece.len()].copy_from_slice(piece);
            self.position = end;
            Ok(piece.len())
        }
    }

    fn all(data: &[u8], chunk: usize, delimiter: u8) -> Vec<Vec<u8>> {
        let mut records = Records::new(
            Chunked {
                data: data.to_vec(),
                position: 0,
                chunk,
            },
            delimiter,
        );
        let mut out = Vec::new();
        while let Some(record) = records.next_record().unwrap() {
            out.push(record.to_vec());
        }
        out
    }

    #[test]
    fn records_split_across_chunks_arrive_whole() {
        assert_eq!(
            all(b"abc\0defgh\0ij\0", 2, 0),
            [b"abc".to_vec(), b"defgh".to_vec(), b"ij".to_vec()]
        );
    }

    #[test]
    fn delimiter_at_the_start_of_a_chunk_ends_the_record() {
        // Chunks: "ab", "\0c", "d\0".
        assert_eq!(all(b"ab\0cd\0", 2, 0), [b"ab".to_vec(), b"cd".to_vec()]);
    }

    #[test]
    fn last_record_without_delimiter_is_returned() {
        assert_eq!(
            all(b"first\nsecond", 3, b'\n'),
            [b"first".to_vec(), b"second".to_vec()]
        );
    }

    #[test]
    fn empty_records_are_kept() {
        assert_eq!(
            all(b"a\0\0b\0", 1, 0),
            [b"a".to_vec(), Vec::new(), b"b".to_vec()]
        );
    }

    #[test]
    fn empty_stream_has_no_records() {
        assert!(all(b"", 4, 0).is_empty());
    }

    #[test]
    fn newline_delimited_records_keep_other_bytes() {
        assert_eq!(
            all(b"a\0b\nc\r\n", 2, b'\n'),
            [b"a\0b".to_vec(), b"c\r".to_vec()]
        );
    }

    #[test]
    fn record_larger_than_the_buffer_arrives_whole() {
        let big = vec![b'x'; 200 * 1024];
        let mut data = big.clone();
        data.push(0);
        data.extend_from_slice(b"tail\0");
        assert_eq!(all(&data, 7000, 0), [big, b"tail".to_vec()]);
    }
}
