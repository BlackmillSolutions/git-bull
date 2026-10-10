//! A branch compared with its base: commits ahead and behind, and the lines
//! it changed since it left the base.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::LineCount;
use gitbull_git::compare::{Counts, FILE_LIMIT, branch_lines, counts};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// `dev` with one commit, and `feature` started from it.
fn dev_and_feature() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("base.txt", "base\n");
    repo.commit("Base");
    repo.git(&["switch", "--quiet", "--create", "dev"]);
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    repo
}

fn lines(count: usize, text: &str) -> String {
    (0..count).map(|n| format!("{text} {n}\n")).collect()
}

#[test]
fn three_ahead_and_one_behind() {
    let mut repo = dev_and_feature();
    for n in 0..3 {
        repo.write(&format!("f{n}.txt"), "x\n");
        repo.commit(&format!("Feature {n}"));
    }
    repo.git(&["switch", "--quiet", "dev"]);
    repo.write("dev.txt", "dev\n");
    repo.commit("Dev");

    let found = counts(&git(), repo.path(), "dev", "feature", &CancelToken::new()).unwrap();
    assert_eq!(
        found,
        Counts {
            ahead: 3,
            behind: 1
        }
    );
}

#[test]
fn a_detached_head_is_compared_by_its_commit() {
    let mut repo = dev_and_feature();
    repo.write("f.txt", "x\n");
    let tip = repo.commit("Feature");
    repo.git(&["switch", "--quiet", "--detach", &tip]);

    let found = counts(&git(), repo.path(), "dev", &tip, &CancelToken::new()).unwrap();
    assert_eq!(
        found,
        Counts {
            ahead: 1,
            behind: 0
        }
    );
}

#[test]
fn lines_since_the_branch_left_its_base() {
    let mut repo = dev_and_feature();
    for n in 0..5 {
        repo.write(&format!("file{n}.txt"), &lines(10, "old"));
    }
    repo.git(&["switch", "--quiet", "dev"]);
    repo.commit("Files on dev");
    repo.git(&["switch", "--quiet", "feature"]);
    repo.git(&["merge", "--quiet", "--ff-only", "dev"]);
    // The base moves on with a change of its own, which must not count.
    repo.git(&["switch", "--quiet", "dev"]);
    repo.write("dev-only.txt", &lines(50, "dev"));
    repo.commit("Dev only");
    repo.git(&["switch", "--quiet", "feature"]);
    // 5 files: 24 new lines each, 8 of their 10 old lines removed.
    for n in 0..5 {
        let mut content = lines(2, "old");
        content.push_str(&lines(24, "new"));
        repo.write(&format!("file{n}.txt"), &content);
    }
    repo.commit("Feature");

    let found = branch_lines(&git(), repo.path(), "dev", "feature", &CancelToken::new())
        .unwrap()
        .unwrap();
    assert_eq!((found.added, found.removed, found.changed), (120, 40, 5));
    assert_eq!(found.files.len(), 5);
    assert!(found.files.iter().all(|file| file.count
        == LineCount::Lines {
            added: 24,
            removed: 8
        }));
    assert_eq!(
        found.merge_base,
        repo.git(&["merge-base", "dev", "feature"]).trim()
    );
}

#[test]
fn many_files_keep_the_first_thousand_and_the_totals_of_all() {
    let mut repo = dev_and_feature();
    for n in 0..1_200 {
        repo.write(&format!("many/f{n:04}.txt"), "one\ntwo\n");
    }
    repo.commit("Many files");

    let found = branch_lines(&git(), repo.path(), "dev", "feature", &CancelToken::new())
        .unwrap()
        .unwrap();
    assert_eq!(found.files.len(), FILE_LIMIT);
    assert_eq!(found.changed, 1_200);
    assert_eq!((found.added, found.removed), (2_400, 0));
}

#[test]
fn branches_without_a_common_commit_have_no_lines() {
    let mut repo = dev_and_feature();
    repo.git(&["switch", "--quiet", "--orphan", "unrelated"]);
    repo.write("other.txt", "other\n");
    repo.commit("Unrelated");

    let found = branch_lines(&git(), repo.path(), "dev", "unrelated", &CancelToken::new()).unwrap();
    assert_eq!(found, None);
}
