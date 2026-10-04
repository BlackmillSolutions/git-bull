//! The web page of a remote (spec `repository-manager`, requirement "Open
//! remote"; design of `worktree-cockpit`, decision 13).

use gitbull_git::facts::RepositoryFacts;

/// The web address of the repository at the remote address `url`:
/// `https://host/path` for an https, scp or ssh address, without a user, a
/// password or a token, and without a trailing `.git`; `None` for an
/// address with a port, another scheme or a local path.
pub fn web_address(url: &str) -> Option<String> {
    let (host, path) = if let Some(rest) = url.strip_prefix("https://") {
        split_authority(rest)?
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        split_authority(rest)?
    } else if url.contains("://") {
        return None;
    } else {
        // `user@host:path`, the form scp uses; a local path has no host
        // before a colon, and a drive letter is no host.
        let (before, path) = url.split_once(':')?;
        let host = before.rsplit_once('@').map_or(before, |(_, host)| host);
        if host.len() < 2 || host.contains(['/', '\\']) || before.is_empty() {
            return None;
        }
        (host.to_owned(), path.trim_start_matches('/').to_owned())
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("https://{host}/{path}"))
}

/// The host and the path of `rest`, an address after its scheme, without a
/// user or password; `None` with a port.
fn split_authority(rest: &str) -> Option<(String, String)> {
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.contains(':') {
        return None;
    }
    Some((host.to_owned(), path.to_owned()))
}

/// The page of `branch`, a branch by its name on the remote, of the
/// repository at the web address `address`, for the hosts whose pages
/// git-bull knows: GitHub and GitLab.
pub fn branch_page(address: &str, branch: &str) -> Option<String> {
    let host = address.strip_prefix("https://")?.split('/').next()?;
    let encoded: Vec<String> = branch.split('/').map(segment).collect();
    let encoded = encoded.join("/");
    match host {
        "github.com" => Some(format!("{address}/tree/{encoded}")),
        "gitlab.com" => Some(format!("{address}/-/tree/{encoded}")),
        _ => None,
    }
}

/// `part` percent-encoded as one segment of a path.
fn segment(part: &str) -> String {
    let mut encoded = String::new();
    for byte in part.bytes() {
        let keep = byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&byte);
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// What Open remote opens for a repository, or for a worktree on the local
/// branch `branch`: the page of its upstream's branch where the host is
/// known, else the page of the repository, at the remote of the upstream,
/// else at `origin`.
pub fn open_remote(facts: &RepositoryFacts, branch: Option<&str>) -> Option<String> {
    let upstream = branch
        .and_then(|branch| facts.branch(&format!("refs/heads/{branch}")))
        .and_then(|branch| branch.upstream.as_ref())
        .filter(|upstream| upstream.remote != ".");
    let remote = upstream.map_or("origin", |upstream| upstream.remote.as_str());
    let address = web_address(&facts.remote(remote)?.url)?;
    let page = upstream.and_then(|upstream| {
        let name = upstream.merge.strip_prefix("refs/heads/")?;
        branch_page(&address, name)
    });
    Some(page.unwrap_or(address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::facts::{Branch, Remote, Upstream};
    use std::path::PathBuf;

    #[test]
    fn an_ssh_remote_opens_its_https_page() {
        assert_eq!(
            web_address("git@github.com:owner/repo.git").as_deref(),
            Some("https://github.com/owner/repo")
        );
        assert_eq!(
            web_address("ssh://git@gitlab.com/group/sub/repo.git").as_deref(),
            Some("https://gitlab.com/group/sub/repo")
        );
    }

    #[test]
    fn credentials_are_left_out() {
        assert_eq!(
            web_address("https://user:token@git.example.com/team/repo.git").as_deref(),
            Some("https://git.example.com/team/repo")
        );
        assert_eq!(
            web_address("https://github.com/owner/repo/").as_deref(),
            Some("https://github.com/owner/repo")
        );
    }

    #[test]
    fn a_port_another_scheme_or_a_local_path_has_no_page() {
        assert_eq!(
            web_address("https://git.example.com:8443/team/repo.git"),
            None
        );
        assert_eq!(
            web_address("ssh://git@example.com:2222/team/repo.git"),
            None
        );
        assert_eq!(web_address("http://example.com/team/repo.git"), None);
        assert_eq!(web_address("git://example.com/team/repo.git"), None);
        assert_eq!(web_address("file:///srv/repo.git"), None);
        assert_eq!(web_address("/srv/git/repo.git"), None);
        assert_eq!(web_address("../repo"), None);
        assert_eq!(web_address(r"C:\work\repo"), None);
        assert_eq!(web_address("C:/work/repo"), None);
    }

    #[test]
    fn a_branch_name_is_encoded_part_by_part() {
        assert_eq!(
            branch_page("https://github.com/owner/repo", "feat#12").as_deref(),
            Some("https://github.com/owner/repo/tree/feat%2312")
        );
        assert_eq!(
            branch_page("https://github.com/owner/repo", "claude/fix reload").as_deref(),
            Some("https://github.com/owner/repo/tree/claude/fix%20reload")
        );
        assert_eq!(
            branch_page("https://gitlab.com/group/repo", "claude/fix").as_deref(),
            Some("https://gitlab.com/group/repo/-/tree/claude/fix")
        );
        assert_eq!(
            branch_page("https://git.example.com/team/repo", "main"),
            None
        );
    }

    fn facts(
        remotes: &[(&str, &str)],
        branches: &[(&str, Option<(&str, &str)>)],
    ) -> RepositoryFacts {
        RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: remotes
                .iter()
                .map(|(name, url)| Remote {
                    name: (*name).to_owned(),
                    url: (*url).to_owned(),
                })
                .collect(),
            common_dir: PathBuf::from("/work/app/.git"),
            branches: branches
                .iter()
                .map(|(name, upstream)| Branch {
                    name: format!("refs/heads/{name}"),
                    commit: "c".to_owned(),
                    upstream: upstream.map(|(remote, merge)| Upstream {
                        tracking: format!("refs/remotes/{remote}/{merge}"),
                        remote: remote.to_owned(),
                        merge: format!("refs/heads/{merge}"),
                    }),
                })
                .collect(),
            origin_head: None,
        }
    }

    #[test]
    fn a_worktree_opens_the_page_of_its_upstream() {
        let found = facts(
            &[("origin", "git@github.com:owner/repo.git")],
            &[("claude/fix-reload", Some(("origin", "claude/fix-reload")))],
        );
        assert_eq!(
            open_remote(&found, Some("claude/fix-reload")).as_deref(),
            Some("https://github.com/owner/repo/tree/claude/fix-reload")
        );
    }

    #[test]
    fn an_upstream_with_another_name_opens_by_its_name_on_the_remote() {
        let found = facts(
            &[("origin", "https://github.com/owner/repo.git")],
            &[("fix", Some(("origin", "claude/fix-reload")))],
        );
        assert_eq!(
            open_remote(&found, Some("fix")).as_deref(),
            Some("https://github.com/owner/repo/tree/claude/fix-reload")
        );
    }

    #[test]
    fn the_remote_of_the_upstream_comes_before_origin() {
        let found = facts(
            &[
                ("origin", "https://github.com/owner/repo.git"),
                ("fork", "git@gitlab.com:me/repo.git"),
            ],
            &[("feature", Some(("fork", "feature")))],
        );
        assert_eq!(
            open_remote(&found, Some("feature")).as_deref(),
            Some("https://gitlab.com/me/repo/-/tree/feature")
        );
    }

    #[test]
    fn without_an_upstream_the_repository_page_of_origin() {
        let found = facts(
            &[("origin", "https://git.example.com/team/repo.git")],
            &[("local", None)],
        );
        assert_eq!(
            open_remote(&found, Some("local")).as_deref(),
            Some("https://git.example.com/team/repo")
        );
        assert_eq!(
            open_remote(&found, None).as_deref(),
            Some("https://git.example.com/team/repo")
        );
    }

    #[test]
    fn a_rewritten_address_opens_as_rewritten() {
        // The facts carry the address after `url.<base>.insteadOf`.
        let found = facts(&[("origin", "git@github.com:owner/repo.git")], &[]);
        assert_eq!(
            open_remote(&found, None).as_deref(),
            Some("https://github.com/owner/repo")
        );
    }

    #[test]
    fn a_local_remote_offers_nothing() {
        let found = facts(&[("origin", "/srv/git/repo.git")], &[]);
        assert_eq!(open_remote(&found, None), None);
        assert_eq!(open_remote(&facts(&[], &[]), None), None);
    }
}
