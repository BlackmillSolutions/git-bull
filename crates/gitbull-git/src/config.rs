//! The listing of the configuration with the scope of each entry, which
//! tells what the user set apart from what the repository brings along
//! (ADR 0006).

/// The arguments of `git config` that list every entry with its scope and
/// its origin, each field ended by NUL. Reading configuration executes
/// nothing.
pub(crate) const LIST: [&str; 5] = ["config", "--list", "--show-scope", "--show-origin", "-z"];

/// The scope, key and value of each entry of a listing of [`LIST`], whose
/// entries read `scope NUL origin NUL key [LF value] NUL`. An entry without
/// a value has an empty one.
pub(crate) fn entries(listing: &[u8]) -> impl Iterator<Item = (&[u8], &[u8], &[u8])> {
    let mut fields = listing.split(|&b| b == 0);
    std::iter::from_fn(move || {
        let (scope, _origin, entry) = (fields.next()?, fields.next()?, fields.next()?);
        let (key, value) = match entry.iter().position(|&b| b == b'\n') {
            Some(newline) => (&entry[..newline], &entry[newline + 1..]),
            None => (entry, &[][..]),
        };
        Some((scope, key, value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_entry_has_its_scope_key_and_value() {
        let listing =
            b"global\0file:C:/Users/ada/.gitconfig\0blame.ignorerevsfile\n~/.blame-ignore\0\
local\0file:.git/config\0core.bare\nfalse\0";
        assert_eq!(
            entries(listing).collect::<Vec<_>>(),
            [
                (
                    &b"global"[..],
                    &b"blame.ignorerevsfile"[..],
                    &b"~/.blame-ignore"[..]
                ),
                (&b"local"[..], &b"core.bare"[..], &b"false"[..]),
            ]
        );
    }

    #[test]
    fn an_entry_without_a_value_has_an_empty_one_and_the_next_follows() {
        let listing = b"local\0file:.git/extra.cfg\0filter.inc.required\0\
worktree\0file:.git/config.worktree\0filter.wt.clean\ncat\0";
        assert_eq!(
            entries(listing).collect::<Vec<_>>(),
            [
                (&b"local"[..], &b"filter.inc.required"[..], &b""[..]),
                (&b"worktree"[..], &b"filter.wt.clean"[..], &b"cat"[..]),
            ]
        );
    }

    #[test]
    fn an_empty_listing_has_no_entries() {
        assert_eq!(entries(b"").count(), 0);
    }
}
