//! Questions about many objects at once, answered by one
//! `git cat-file --batch-check` process.

use std::io::{Read, Write};
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;

/// The answers of `git cat-file --batch-check=<format>` to `requests`, one
/// line each, in their order.
///
/// The requests are written on a thread of their own while the answers are
/// read: Git stops reading requests once its answers fill the output pipe,
/// so writing all before reading any would wait forever.
pub fn batch_check(
    git: &Git,
    repo: &Path,
    format: &str,
    requests: &[String],
    cancel: &CancelToken,
) -> Result<Vec<u8>, Error> {
    let args = ["cat-file".to_owned(), format!("--batch-check={format}")];
    let mut process = git.spawn(repo, &[], &args, true)?;
    let command = process.command().to_owned();
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut stdin = process.take_stdin().expect("standard input is piped");
    let input: String = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect();
    // Dropping the input at the end tells Git that no more requests come.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let mut output = Vec::new();
    let read = process
        .take_stdout()
        .expect("standard output is piped")
        .read_to_end(&mut output);
    let written = writer.join().expect("writing the requests does not panic");
    let result = process.wait();
    cancel.forget(registration);
    result?;
    let io = |source| Error::Io {
        command: command.clone(),
        source,
    };
    written.map_err(io)?;
    read.map_err(io)?;
    Ok(output)
}
