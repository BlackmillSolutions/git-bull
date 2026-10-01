//! The content of a version of a file, for syntax highlighting.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;
use crate::object_id::ObjectId;

const COMMAND: &str = "git cat-file --batch";

/// The content of `blob`, or `None` when it is larger than `limit` bytes;
/// such a blob is not read.
pub fn blob(
    git: &Git,
    repo: &Path,
    blob: &ObjectId,
    limit: u64,
    cancel: &CancelToken,
) -> Result<Option<Vec<u8>>, Error> {
    let mut process = git.spawn(repo, &[], ["cat-file", "--batch"], true)?;
    let canceller = process.canceller();
    let stop = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut stdin = process.take_stdin().expect("standard input is piped");
    let written = stdin.write_all(format!("{blob}\n").as_bytes());
    // Closing the input tells Git that no more requests come.
    drop(stdin);
    let mut reader = BufReader::new(process.take_stdout().expect("standard output is piped"));
    let read = read_blob(&mut reader, limit);
    let too_large = matches!(read, Ok(Ok(None)));
    if too_large {
        // The content is not needed; Git ends when it is stopped.
        stop.cancel();
    }
    let result = process.wait();
    cancel.forget(registration);
    if !too_large {
        result?;
    }
    let io = |source| Error::Io {
        command: COMMAND.to_owned(),
        source,
    };
    written.map_err(io)?;
    read.map_err(io)?.map_err(|message| Error::Parse {
        command: COMMAND.to_owned(),
        message,
        bytes: Vec::new(),
    })
}

/// Reads one answer of `git cat-file --batch`: `<id> <type> <size>`, then
/// the content, or `<id> missing`.
fn read_blob(reader: &mut impl BufRead, limit: u64) -> io::Result<Result<Option<Vec<u8>>, String>> {
    let mut header = String::new();
    reader.read_line(&mut header)?;
    let fields: Vec<&str> = header.trim_end().split(' ').collect();
    let size = match fields[..] {
        [_, "blob", size] => match size.parse::<u64>() {
            Ok(size) => size,
            Err(_) => return Ok(Err(format!("expected a size: {header:?}"))),
        },
        [id, "missing"] => return Ok(Err(format!("the object {id} is missing"))),
        _ => return Ok(Err(format!("expected a blob: {header:?}"))),
    };
    if size > limit {
        return Ok(Ok(None));
    }
    let mut content = Vec::with_capacity(size as usize);
    reader.take(size).read_to_end(&mut content)?;
    if content.len() as u64 != size {
        return Ok(Err(format!("expected {size} bytes, got {}", content.len())));
    }
    Ok(Ok(Some(content)))
}
