//! Builds large repositories for benchmarks through `git fast-import`.
//!
//! The history resembles a project: a main line into which short topic
//! branches are merged, a tag every so many commits, a few branches and a
//! remote branch. The same shape always gives the same history.

use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// What the generated history looks like.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    /// Commits in all, merges included.
    pub commits: usize,
    /// A topic branch starts after this many commits on the main line.
    pub topic_every: usize,
    /// Commits on each topic branch before it is merged.
    pub topic_length: usize,
    /// A tag after this many commits on the main line.
    pub tag_every: usize,
}

impl Shape {
    /// `commits` commits in the shape the benchmarks use.
    pub fn benchmark(commits: usize) -> Shape {
        Shape {
            commits,
            topic_every: 20,
            topic_length: 4,
            tag_every: 1_000,
        }
    }
}

/// The first commit date; each commit is a minute later.
const FIRST_DATE: u64 = 1_600_000_000;

/// Creates a repository at `dir`, which must not exist yet or be empty,
/// with the history `shape` describes, checked out at `main`.
pub fn generate(dir: &Path, shape: &Shape) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    run(git(dir).args(["init", "--quiet", "--initial-branch=main"]))?;
    let mut child = git(dir)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()?;
    // Written while Git reads it, so that a million commits need not fit
    // in memory at once.
    let mut stream = BufWriter::new(child.stdin.take().expect("standard input is piped"));
    History::new(*shape).write(&mut stream)?;
    stream.flush()?;
    drop(stream);
    if !child.wait()?.success() {
        return Err(io::Error::other("git fast-import failed"));
    }
    run(git(dir).args(["reset", "--quiet", "--hard", "main"]))
}

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir).env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

fn run(command: &mut Command) -> io::Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

/// The state of the stream being written.
struct History {
    shape: Shape,
    /// The last mark given, and the commits written.
    mark: usize,
    created: usize,
    main_tip: Option<usize>,
    main_count: usize,
    /// Commits of the main line kept for branches made at the end.
    halfway: Option<usize>,
    near_end: Option<usize>,
}

impl History {
    fn new(shape: Shape) -> History {
        History {
            shape,
            mark: 0,
            created: 0,
            main_tip: None,
            main_count: 0,
            halfway: None,
            near_end: None,
        }
    }

    fn write(mut self, out: &mut impl Write) -> io::Result<()> {
        let shape = self.shape;
        while self.created < shape.commits {
            let room = shape.commits - self.created;
            let topic = self.main_count > 0
                && self.main_count.is_multiple_of(shape.topic_every)
                && room > shape.topic_length;
            let parents = if topic {
                // A topic branch forks from the main line and is merged
                // back into it.
                let mut tip = self.main_tip.expect("the main line has commits");
                for _ in 0..shape.topic_length {
                    tip = self.commit(out, "refs/heads/topic", &[tip])?;
                }
                vec![self.main_tip.expect("the main line has commits"), tip]
            } else {
                self.main_tip.into_iter().collect()
            };
            let tip = self.commit(out, "refs/heads/main", &parents)?;
            self.main_tip = Some(tip);
            self.main_count += 1;
            if self.main_count.is_multiple_of(shape.tag_every) {
                let tag = format!("refs/tags/v{}", self.main_count / shape.tag_every);
                reset(out, &tag, tip)?;
            }
            if self.halfway.is_none() && self.created >= shape.commits / 2 {
                self.halfway = Some(tip);
            }
            if self.near_end.is_none() && self.created >= shape.commits / 10 * 9 {
                self.near_end = Some(tip);
            }
        }
        if let Some(tip) = self.main_tip {
            reset(out, "refs/remotes/origin/main", tip)?;
            reset(out, "refs/heads/feature/old", self.halfway.unwrap_or(tip))?;
            reset(out, "refs/heads/release", self.near_end.unwrap_or(tip))?;
        }
        Ok(())
    }

    /// Writes a commit on `reference` that changes one of a hundred files,
    /// and returns its mark.
    fn commit(
        &mut self,
        out: &mut impl Write,
        reference: &str,
        parents: &[usize],
    ) -> io::Result<usize> {
        self.mark += 1;
        let n = self.created;
        let date = FIRST_DATE + 60 * n as u64;
        let message = format!("Commit {n}\n");
        let content = format!("change {n}\n");
        writeln!(out, "commit {reference}")?;
        writeln!(out, "mark :{}", self.mark)?;
        writeln!(out, "committer Ada Lovelace <ada@example.com> {date} +0000")?;
        writeln!(out, "data {}", message.len())?;
        out.write_all(message.as_bytes())?;
        if let Some((first, merged)) = parents.split_first() {
            writeln!(out, "from :{first}")?;
            for parent in merged {
                writeln!(out, "merge :{parent}")?;
            }
        }
        writeln!(out, "M 644 inline src/file{:02}.txt", n % 100)?;
        writeln!(out, "data {}", content.len())?;
        out.write_all(content.as_bytes())?;
        writeln!(out)?;
        self.created += 1;
        Ok(self.mark)
    }
}

/// Points `reference` at the commit with `mark`.
fn reset(out: &mut impl Write, reference: &str, mark: usize) -> io::Result<()> {
    writeln!(out, "reset {reference}")?;
    writeln!(out, "from :{mark}")?;
    writeln!(out)
}
