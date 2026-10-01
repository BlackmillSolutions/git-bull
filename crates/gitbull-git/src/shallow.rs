//! Where the history of a shallow clone ends.

use std::path::Path;

use crate::error::Error;
use crate::invoke::Git;
use crate::object_id::ObjectId;

/// The commits at which the available history ends. Git lists them in the
/// `shallow` file; a repository that is not shallow has none.
pub fn shallow_commits(git: &Git, repo: &Path) -> Result<Vec<ObjectId>, Error> {
    // Asking Git finds the file of worktrees and of other layouts too.
    let output = git.run(repo, &[], ["rev-parse", "--git-path", "shallow"])?;
    let command = "git rev-parse --git-path shallow";
    let path = String::from_utf8(output).map_err(|e| Error::Parse {
        command: command.to_owned(),
        message: "the path is not UTF-8".to_owned(),
        bytes: e.into_bytes(),
    })?;
    let path = repo.join(path.trim_end_matches(['\r', '\n']));
    let contents = match std::fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Io {
                command: format!("reading {}", path.display()),
                source,
            });
        }
    };
    parse_shallow(&contents).map_err(|message| Error::Parse {
        command: format!("reading {}", path.display()),
        message,
        bytes: contents,
    })
}

/// Reads the `shallow` file: one full object name per line.
fn parse_shallow(contents: &[u8]) -> Result<Vec<ObjectId>, String> {
    contents
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty())
        .map(|line| {
            ObjectId::from_hex(line).ok_or_else(|| {
                format!(
                    "expected an object name: {:?}",
                    String::from_utf8_lossy(line)
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: &str = "1111111111111111111111111111111111111111";
    const C2: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn every_line_is_a_commit() {
        let contents = format!("{C1}\n{C2}\n");
        let ids = parse_shallow(contents.as_bytes()).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[1].to_string(), C2);
    }

    #[test]
    fn an_empty_file_lists_none() {
        assert!(parse_shallow(b"").unwrap().is_empty());
    }

    #[test]
    fn a_line_that_is_no_object_name_is_an_error() {
        assert!(parse_shallow(b"not a hash\n").is_err());
    }
}
