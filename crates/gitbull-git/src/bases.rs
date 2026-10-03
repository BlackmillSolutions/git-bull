//! The branch a branch most likely started from, as Git 2.47 and newer
//! tell it with `%(is-base)` (design of `worktree-cockpit`, decision 3).

use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;

/// Above this many branches to leave out, the command line could grow too
/// long for Windows, and nothing is detected.
const EXCLUDE_LIMIT: usize = 500;

/// The full name of the branch that `tip` most likely started from, among
/// the local and remote-tracking branches, or `None` when Git marks none.
///
/// Left out are `refs/remotes/*/HEAD` and every branch that already
/// contains `tip`, as `tip` itself, its remote-tracking branches, a branch
/// stacked on it and one that merged it; the `integration` branches, given
/// by their full names in the order of preference, stay candidates. When the
/// branch Git marks is no integration branch, an integration branch that
/// leaves `tip` at the same commit (`git merge-base`) is taken instead, as
/// branches that started from the same commit tie.
pub fn detect_base(
    git: &Git,
    repo: &Path,
    tip: &str,
    integration: &[String],
    cancel: &CancelToken,
) -> Result<Option<String>, Error> {
    let containing = format!("--contains={tip}");
    let output = git.run_cancellable(
        repo,
        &[],
        [
            "for-each-ref",
            "--format=%(refname)",
            containing.as_str(),
            "refs/heads",
            "refs/remotes",
        ],
        cancel,
    )?;
    let excluded: Vec<String> = String::from_utf8_lossy(&output)
        .lines()
        .filter(|name| !name.is_empty() && !integration.iter().any(|kept| kept == name))
        .map(|name| format!("--exclude={name}"))
        .collect();
    if excluded.len() > EXCLUDE_LIMIT {
        return Ok(None);
    }
    let format = format!("--format=%(refname)%00%(is-base:{tip})");
    let mut args = vec![
        "for-each-ref".to_owned(),
        format,
        "--exclude=refs/remotes/*/HEAD".to_owned(),
    ];
    args.extend(excluded);
    args.extend(["refs/heads".to_owned(), "refs/remotes".to_owned()]);
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    let Some(marked) = marked(&output) else {
        return Ok(None);
    };
    if integration.contains(&marked) {
        return Ok(Some(marked));
    }
    let Some(left_at) = merge_base(git, repo, tip, &marked, cancel)? else {
        return Ok(Some(marked));
    };
    for candidate in integration {
        if merge_base(git, repo, tip, candidate, cancel)?.as_deref() == Some(&left_at) {
            return Ok(Some(candidate.clone()));
        }
    }
    Ok(Some(marked))
}

/// The ref that `%(is-base)` marks: the line whose second field is not
/// empty.
fn marked(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output).lines().find_map(|line| {
        let (name, mark) = line.split_once('\0')?;
        (!mark.is_empty()).then(|| name.to_owned())
    })
}

/// The commit where `a` and `b` meet, or `None` when they never do.
pub fn merge_base(
    git: &Git,
    repo: &Path,
    a: &str,
    b: &str,
    cancel: &CancelToken,
) -> Result<Option<String>, Error> {
    match git.run_cancellable(repo, &[], ["merge-base", "--end-of-options", a, b], cancel) {
        Ok(output) => Ok(Some(String::from_utf8_lossy(&output).trim().to_owned())),
        // Git exits with 1 when the two have no common commit.
        Err(Error::CommandFailed { code: Some(1), .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marked_ref_is_the_one_with_a_mark() {
        let output =
            b"refs/heads/claude/b\x00\nrefs/heads/dev\x00(claude/a)\nrefs/heads/main\x00\n";
        assert_eq!(marked(output).as_deref(), Some("refs/heads/dev"));
    }

    #[test]
    fn without_a_mark_nothing_is_marked() {
        assert_eq!(marked(b"refs/heads/dev\x00\nrefs/heads/main\x00\n"), None);
        assert_eq!(marked(b""), None);
    }
}
