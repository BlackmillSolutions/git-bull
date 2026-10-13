---
status: accepted
date: 2026-10-12
---

# Discarded work is kept in a journal

Discarding changes of the working copy deletes work that no commit holds.
Git cannot bring it back, and an application that offers the action must. Git
also makes the user's wish for an exact way back hard: `git stash` and `git
restore` read the index, convert line endings, and know nothing of untracked
files. This record decides where discarded work goes, how it comes back, and
what the code may never do to a byte of the user's.

## Decision

Every discard keeps what it takes in a **journal**, a folder in the Git folder
of the worktree (`git rev-parse --git-path gitbull/discards`, so a linked
worktree has its own). One entry per discard holds a `manifest`, a text file
written and read by git-bull alone, and a numbered **slot** per file: the file
that the working copy had at that path, with its exact bytes, mode and times.
Nothing is written to the journal before the discard, and the discard changes
nothing before the entry exists.

- **A move, not a copy.** A whole file is moved into its slot with a rename:
  atomic, free for a large file, and nothing of it is read or converted. Git
  then writes tracked files again from the index. Untracked files need no
  second step: the move is their discard, so `git clean` is never used. A
  rename that crosses volumes becomes a copy to a `.part` file that is renamed
  when it is complete. A hunk or lines are the one case of a copy: the file is
  copied into its slot, and the reverse patch is applied to the file in place.
  A line check against the copy then finds a line outside the hunks that Git
  rewrote, as when it converts line endings on writing, and puts the copy
  back.
- **Not Git's objects, not application data.** Objects under a reference, or a
  stash made with `git stash create`, would not be exact (filters and line
  endings are applied), would be pushed by `push --mirror`, and a stash has no
  place for untracked files. A folder in the application's data does not move
  with the repository, has no place per worktree, and can lie on another
  volume, where a move is a copy.
- **States and a lock of the operating system.** An entry is `pending` while a
  discard moves its files, `discarded` when done, `restoring` or `redoing`
  while an exchange runs, and `restored`. A `.lock` file is locked with the
  advisory lock of the operating system for the whole of an operation. A
  process that ends releases the lock at once, so a reader tells an entry that
  is **running** from one that is **interrupted** by trying the lock, and two
  instances of git-bull on one worktree take turns. A Git folder on a file
  system without locks refuses the discard: no journal, no discard.
- **Restoring is an exchange classified by identity.** Restore, undo and
  "Discard again" swap, for each file, what the path holds and what the slot
  holds, in three renames that never go onto a name that exists. The manifest
  first records the identity (size, time in nanoseconds, inode or creation
  time) of both files, and a rename keeps identity, so after an end of the
  process every file can be found at one of four steps and the exchange goes
  on. A file of any other identity is left alone and named. Restoring never
  destroys: what lay at the path is in the slot afterwards.
- **Rules that overwrite nothing.** An untracked file is put back only where
  its path is absent. A tracked file is put back where its path is absent or
  holds what the index holds, found by hashing it as Git would. A path that
  holds anything else is left alone, named, and its entry stays.
- **Paths are checked first.** No path may lie in the Git folder, go through a
  symbolic link, name a repository of its own, or have a folder where a file
  is expected. Every level is looked at with `symlink_metadata`, and an entry
  that is a link, or a slot that is not a file, is listed as unreadable and
  never followed: the journal is data on disk that another program could have
  planted.
- **Retention.** At the next discard, with the lock, entries older than 30
  days are removed, then the oldest while the journal holds more than 1 GiB.
  Never the entry just made, never an interrupted one. "Clear" removes every
  entry. These and the removal of the worktree are the only places that delete
  a byte of the journal.

## Consequences

- Removing a linked worktree removes its journal with it. Git does that, and
  git-bull cannot hold it back.
- A program that holds a moved file open keeps writing to it in its slot; the
  entry changes after the discard, and its size is stale.
- A Git that a discard started can outlive git-bull. The manifest records its
  process id, and putting the entry back waits while that process lives, but
  trusts the id only for an hour: it may have gone to another program.
- Only the end of the process is survived, not the end of the machine: the
  journal is not synced to the disk.
- A program that writes to a file in the moments between the move and Git's
  write can lose that write. The check before Git runs narrows the time and
  the rules above never overwrite what they find, but nothing closes it.
- The identity of a file is its size, its time of modification and its inode,
  or on Windows its time of creation. Two files of one entry that agree in
  all of these are not told apart where the clock is coarse, so a resume
  that finds them may leave a file alone and name it; it never overwrites.
- A file that was exchanged can hold stale bytes: a tracked file restored
  holds the index of the time. They are not lost, and the file shows as
  modified.

Alternatives considered: Git objects under a hidden reference, a stash, a copy
into the application's data, and a restore that makes an entry of its own,
which would put two entries in the list for each undo.
