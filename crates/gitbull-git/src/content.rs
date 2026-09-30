//! Commit content from one persistent `git cat-file --batch` process
//! (design, decision 5).
//!
//! A writer thread sends requests and a reader thread reads responses, so
//! requests are pipelined. At most [`MAX_OUTSTANDING`] are in flight: writing
//! all requests before reading any response would deadlock as soon as the
//! output pipe is full.

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::thread;
use std::time::Duration;

use crate::error::Error;
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::process::Process;

/// Requests written to Git whose response has not been read yet.
pub const MAX_OUTSTANDING: usize = 256;

const COMMAND: &str = "git cat-file --batch";

/// A person and a point in time, as in the author and committer headers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// Seconds since 1970.
    pub time: i64,
    /// The time zone of `time`, in minutes east of UTC.
    pub offset_minutes: i32,
}

/// What the commit list and the details show of a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitContent {
    pub author: Signature,
    pub committer: Signature,
    /// The whole message, decoded from the encoding the commit declares.
    pub message: String,
}

/// The response to one request.
#[derive(Debug)]
pub struct Content {
    pub id: ObjectId,
    pub result: Result<CommitContent, Error>,
}

/// Reads commit content on two threads of its own. Dropping it stops Git.
pub struct ContentReader {
    queue: Sender<ObjectId>,
    results: Receiver<Content>,
    _process: Process,
}

impl ContentReader {
    /// Starts `git cat-file --batch`. `notify` runs after every response.
    pub fn start(
        git: &Git,
        repo: &Path,
        notify: impl Fn() + Send + 'static,
    ) -> Result<ContentReader, Error> {
        let mut process = git.spawn(repo, &[], ["cat-file", "--batch"], true)?;
        let stdin = process.take_stdin().expect("standard input is piped");
        let stdout = process.take_stdout().expect("standard output is piped");
        let (queue, results) = pipeline(stdin, stdout, Box::new(notify));
        Ok(ContentReader {
            queue,
            results,
            _process: process,
        })
    }

    /// Queues requests; never blocks.
    pub fn request(&self, ids: impl IntoIterator<Item = ObjectId>) {
        for id in ids {
            // The reader threads have ended only when Git has; nothing to do.
            let _ = self.queue.send(id);
        }
    }

    /// The next response, if one has arrived.
    pub fn try_next(&self) -> Option<Content> {
        self.results.try_recv().ok()
    }

    /// The next response, waiting at most `timeout`.
    pub fn next_timeout(&self, timeout: Duration) -> Option<Content> {
        self.results.recv_timeout(timeout).ok()
    }
}

/// Starts the writer and the reader thread on the pipes of a `cat-file`.
fn pipeline<W, R>(
    stdin: W,
    stdout: R,
    notify: Box<dyn Fn() + Send>,
) -> (Sender<ObjectId>, Receiver<Content>)
where
    W: Write + Send + 'static,
    R: Read + Send + 'static,
{
    let (queue, requests) = mpsc::channel();
    // The reader holds one request while it reads the response, so the
    // channel holds one fewer.
    let (expect, expected) = mpsc::sync_channel(MAX_OUTSTANDING - 1);
    let (results, responses) = mpsc::channel();
    thread::Builder::new()
        .name("cat-file writer".into())
        .spawn(move || write_requests(stdin, requests, expect))
        .expect("thread for the cat-file writer");
    thread::Builder::new()
        .name("cat-file reader".into())
        .spawn(move || read_responses(stdout, expected, results, notify))
        .expect("thread for the cat-file reader");
    (queue, responses)
}

fn write_requests(stdin: impl Write, requests: Receiver<ObjectId>, expect: SyncSender<ObjectId>) {
    let mut out = BufWriter::new(stdin);
    loop {
        // Flush before anything that may block, so that Git sees every
        // request the reader waits for.
        let id = match requests.try_recv() {
            Ok(id) => id,
            Err(TryRecvError::Empty) => {
                if out.flush().is_err() {
                    return;
                }
                match requests.recv() {
                    Ok(id) => id,
                    Err(_) => return,
                }
            }
            Err(TryRecvError::Disconnected) => {
                let _ = out.flush();
                return;
            }
        };
        match expect.try_send(id) {
            Ok(()) => {}
            Err(TrySendError::Full(id)) => {
                if out.flush().is_err() || expect.send(id).is_err() {
                    return;
                }
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
        if writeln!(out, "{id}").is_err() {
            return;
        }
    }
}

fn read_responses(
    stdout: impl Read,
    expected: Receiver<ObjectId>,
    results: Sender<Content>,
    notify: Box<dyn Fn() + Send>,
) {
    let mut input = BufReader::new(stdout);
    let mut header = Vec::new();
    while let Ok(id) = expected.recv() {
        header.clear();
        match input.read_until(b'\n', &mut header) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let Ok(result) = read_object(&mut input, &header) else {
            return;
        };
        if results.send(Content { id, result }).is_err() {
            return;
        }
        notify();
    }
}

/// Reads the object after its header line `<id> <type> <size>`, or nothing
/// after `<id> missing`. Errors of the pipe end the reader.
fn read_object(
    input: &mut impl BufRead,
    header: &[u8],
) -> std::io::Result<Result<CommitContent, Error>> {
    let text = String::from_utf8_lossy(header);
    let fields: Vec<&str> = text.split_whitespace().collect();
    let unexpected = |message: String| Error::Parse {
        command: COMMAND.to_owned(),
        message,
        bytes: header.to_vec(),
    };
    let (kind, size) = match fields[..] {
        [_, kind, size] => match size.parse::<usize>() {
            Ok(size) => (kind, size),
            Err(_) => return Ok(Err(unexpected("expected a size".into()))),
        },
        [_, "missing"] => return Ok(Err(unexpected("the object is missing".into()))),
        _ => return Ok(Err(unexpected("expected an object header".into()))),
    };
    // The object and the newline after it.
    let mut body = vec![0; size + 1];
    input.read_exact(&mut body)?;
    body.pop();
    if kind == "commit" {
        Ok(Ok(parse_commit(&body)))
    } else {
        Ok(Err(unexpected(format!("expected a commit, got a {kind}"))))
    }
}

/// Reads a raw commit object. Never fails: fields that cannot be read stay
/// empty, so that one odd commit does not hide the rest.
pub fn parse_commit(raw: &[u8]) -> CommitContent {
    let (headers, message) = match raw.windows(2).position(|w| w == b"\n\n") {
        Some(end) => (&raw[..end], &raw[end + 2..]),
        None => (raw, &b""[..]),
    };
    let mut author = &b""[..];
    let mut committer = &b""[..];
    let mut encoding = None;
    // Continuation lines of multi-line headers such as gpgsig start with a
    // space and never match a header name.
    for line in headers.split(|&b| b == b'\n') {
        if let Some(value) = line.strip_prefix(b"author ") {
            author = value;
        } else if let Some(value) = line.strip_prefix(b"committer ") {
            committer = value;
        } else if let Some(value) = line.strip_prefix(b"encoding ") {
            encoding = encoding_rs::Encoding::for_label(value.trim_ascii());
        }
    }
    let decode = |bytes: &[u8]| match encoding {
        Some(encoding) => encoding.decode_without_bom_handling(bytes).0.into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    };
    CommitContent {
        author: parse_signature(author, &decode),
        committer: parse_signature(committer, &decode),
        message: decode(message),
    }
}

/// Reads `Name <email> <time> <zone>`.
fn parse_signature(value: &[u8], decode: &dyn Fn(&[u8]) -> String) -> Signature {
    let (Some(open), Some(close)) = (
        value.iter().position(|&b| b == b'<'),
        value.iter().rposition(|&b| b == b'>'),
    ) else {
        return Signature {
            name: decode(value.trim_ascii()),
            ..Signature::default()
        };
    };
    let name = decode(value[..open].trim_ascii());
    let email = decode(value.get(open + 1..close).unwrap_or_default());
    let rest = String::from_utf8_lossy(&value[close + 1..]).into_owned();
    let mut fields = rest.split_whitespace();
    let time = fields.next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let offset_minutes = fields.next().and_then(parse_zone).unwrap_or(0);
    Signature {
        name,
        email,
        time,
        offset_minutes,
    }
}

/// Reads `+0200` or `-0530` as minutes east of UTC.
fn parse_zone(zone: &str) -> Option<i32> {
    let (sign, digits) = match zone.as_bytes().first()? {
        b'+' => (1, &zone[1..]),
        b'-' => (-1, &zone[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    Some(sign * (hours * 60 + minutes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    const RAW: &[u8] = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
parent 1111111111111111111111111111111111111111\n\
author Ada Lovelace <ada@example.com> 1767268800 +0200\n\
committer Grace Hopper <grace@example.com> 1767272400 -0530\n\
\n\
Fix the parser\n\
\n\
The body.\n";

    #[test]
    fn signatures_and_message_are_read() {
        let content = parse_commit(RAW);
        assert_eq!(
            content.author,
            Signature {
                name: "Ada Lovelace".into(),
                email: "ada@example.com".into(),
                time: 1_767_268_800,
                offset_minutes: 120,
            }
        );
        assert_eq!(content.committer.name, "Grace Hopper");
        assert_eq!(content.committer.offset_minutes, -330);
        assert_eq!(content.message, "Fix the parser\n\nThe body.\n");
    }

    #[test]
    fn signature_lines_are_skipped_with_their_continuation_lines() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author A <a@example.com> 1 +0000\n\
committer C <c@example.com> 2 +0000\n\
gpgsig -----BEGIN PGP SIGNATURE-----\n \n abc\n -----END PGP SIGNATURE-----\n\
\n\
Signed\n";
        let content = parse_commit(raw);
        assert_eq!(content.committer.name, "C");
        assert_eq!(content.message, "Signed\n");
    }

    #[test]
    fn declared_iso_8859_1_is_decoded_in_message_and_names() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author J\xf6rg <j@example.com> 1 +0000\n\
committer J\xf6rg <j@example.com> 1 +0000\n\
encoding ISO-8859-1\n\
\n\
Gr\xfc\xdfe\n";
        let content = parse_commit(raw);
        assert_eq!(content.message, "Grüße\n");
        assert_eq!(content.author.name, "Jörg");
    }

    #[test]
    fn invalid_utf_8_without_encoding_gets_replacement_characters() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author A <a@example.com> 1 +0000\n\
committer C <c@example.com> 1 +0000\n\
\n\
Hello \xff world\n";
        assert_eq!(parse_commit(raw).message, "Hello \u{FFFD} world\n");
    }

    #[test]
    fn unknown_encoding_is_read_as_utf_8() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author A <a@example.com> 1 +0000\n\
committer C <c@example.com> 1 +0000\n\
encoding no-such-encoding\n\
\n\
Gr\xc3\xbc\xc3\x9fe\n";
        assert_eq!(parse_commit(raw).message, "Grüße\n");
    }

    #[test]
    fn commit_without_message_has_an_empty_message() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author A <a@example.com> 1 +0000\n\
committer C <c@example.com> 1 +0000\n";
        let content = parse_commit(raw);
        assert_eq!(content.message, "");
        assert_eq!(content.committer.name, "C");
    }

    #[test]
    fn odd_signature_is_read_as_far_as_possible() {
        let raw = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author Nobody\n\
committer C <c@example.com> not-a-time\n\
\n\
Odd\n";
        let content = parse_commit(raw);
        assert_eq!(content.author.name, "Nobody");
        assert_eq!(content.author.email, "");
        assert_eq!(content.committer.email, "c@example.com");
        assert_eq!(content.committer.time, 0);
        assert_eq!(content.message, "Odd\n");
    }

    /// Standard input of a fake `cat-file` that counts the requests.
    #[derive(Clone, Default)]
    struct CountingInput {
        lines: Arc<AtomicUsize>,
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for CountingInput {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let newlines = buf.iter().filter(|&&b| b == b'\n').count();
            self.bytes.lock().unwrap().extend_from_slice(buf);
            self.lines.fetch_add(newlines, Ordering::SeqCst);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn wait_for(lines: &AtomicUsize, count: usize) {
        let started = Instant::now();
        while lines.load(Ordering::SeqCst) < count {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "only {} requests written",
                lines.load(Ordering::SeqCst)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn id(n: u32) -> ObjectId {
        let mut bytes = [0; 20];
        bytes[..4].copy_from_slice(&n.to_be_bytes());
        ObjectId::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn at_most_256_requests_are_outstanding() {
        let input = CountingInput::default();
        let (output, mut responses) = std::io::pipe().unwrap();
        let (queue, results) = pipeline(input.clone(), output, Box::new(|| {}));
        for n in 0..1000 {
            queue.send(id(n)).unwrap();
        }

        wait_for(&input.lines, MAX_OUTSTANDING);
        thread::sleep(Duration::from_millis(200));
        assert_eq!(input.lines.load(Ordering::SeqCst), MAX_OUTSTANDING);

        // One response lets exactly one more request through.
        let body = String::from_utf8_lossy(RAW).into_owned();
        write!(responses, "{} commit {}\n{body}\n", id(0), RAW.len()).unwrap();
        let first = results.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(first.id, id(0));
        assert_eq!(first.result.unwrap().author.name, "Ada Lovelace");
        wait_for(&input.lines, MAX_OUTSTANDING + 1);
        thread::sleep(Duration::from_millis(200));
        assert_eq!(input.lines.load(Ordering::SeqCst), MAX_OUTSTANDING + 1);

        let written = input.bytes.lock().unwrap().clone();
        let first_line = written.split(|&b| b == b'\n').next().unwrap();
        assert_eq!(first_line, id(0).to_string().as_bytes());
    }

    #[test]
    fn missing_objects_and_other_types_are_errors() {
        let input = CountingInput::default();
        let (output, mut responses) = std::io::pipe().unwrap();
        let (queue, results) = pipeline(input, output, Box::new(|| {}));
        queue.send(id(1)).unwrap();
        queue.send(id(2)).unwrap();
        queue.send(id(3)).unwrap();
        writeln!(responses, "{} missing", id(1)).unwrap();
        write!(responses, "{} blob 5\nhello\n", id(2)).unwrap();
        let body = String::from_utf8_lossy(RAW).into_owned();
        write!(responses, "{} commit {}\n{body}\n", id(3), RAW.len()).unwrap();

        let next = || results.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(next().result.is_err());
        assert!(next().result.is_err());
        let third = next();
        assert_eq!(third.id, id(3));
        assert!(third.result.is_ok());
    }
}
