//! Optional repository statistics, read from `git` if it is on PATH and the
//! scan root is inside a work tree. Every field degrades to "unavailable"
//! rather than failing the run.

use std::path::Path;
use std::process::Command;

pub struct GitStats {
    pub branch: String,
    pub commits: u64,
    pub authors: u64,
    pub first_commit: String,
    pub last_commit: String,
    pub active_days: u64,
    pub top_authors: Vec<(String, u64)>,
    pub uncommitted: u64,
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl GitStats {
    /// Returns `None` when the directory is not a git work tree or git is absent.
    pub fn collect(root: &Path) -> Option<GitStats> {
        if git(root, &["rev-parse", "--is-inside-work-tree"])? != "true" {
            return None;
        }

        let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "detached".into());
        let commits = git(root, &["rev-list", "--count", "HEAD"]).and_then(|s| s.parse().ok()).unwrap_or(0);

        let shortlog = git(root, &["shortlog", "-sne", "--all", "HEAD"]).unwrap_or_default();
        let mut top_authors: Vec<(String, u64)> = shortlog
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let (count, rest) = l.split_once('\t').or_else(|| l.split_once(' '))?;
                let name = rest.split('<').next()?.trim().to_string();
                Some((name, count.trim().parse().ok()?))
            })
            .collect();
        top_authors.sort_by(|a, b| b.1.cmp(&a.1));
        let authors = top_authors.len() as u64;
        top_authors.truncate(5);

        let dates = git(root, &["log", "--pretty=format:%ad", "--date=short"]).unwrap_or_default();
        let mut days: Vec<&str> = dates.lines().collect();
        let last_commit = days.first().unwrap_or(&"unknown").to_string();
        let first_commit = days.last().unwrap_or(&"unknown").to_string();
        days.sort_unstable();
        days.dedup();
        let active_days = days.len() as u64;

        let uncommitted = git(root, &["status", "--porcelain"])
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u64)
            .unwrap_or(0);

        Some(GitStats {
            branch,
            commits,
            authors,
            first_commit,
            last_commit,
            active_days,
            top_authors,
            uncommitted,
        })
    }

    pub fn rows(&self) -> Vec<(String, String)> {
        let mut r = vec![
            ("Branch".into(), self.branch.clone()),
            ("Commits".into(), crate::report::group_digits(self.commits)),
            ("Contributors".into(), crate::report::group_digits(self.authors)),
            ("First commit".into(), self.first_commit.clone()),
            ("Last commit".into(), self.last_commit.clone()),
            ("Days with commits".into(), crate::report::group_digits(self.active_days)),
            ("Uncommitted changes".into(), crate::report::group_digits(self.uncommitted)),
        ];
        for (i, (name, n)) in self.top_authors.iter().enumerate() {
            r.push((format!("Top contributor #{}", i + 1), format!("{name} ({} commits)", crate::report::group_digits(*n))));
        }
        r
    }

    pub fn to_json(&self) -> String {
        let authors: Vec<String> = self
            .top_authors
            .iter()
            .map(|(n, c)| format!(r#"{{"name":"{}","commits":{}}}"#, json_escape(n), c))
            .collect();
        format!(
            r#"{{"branch":"{}","commits":{},"contributors":{},"first_commit":"{}","last_commit":"{}","active_days":{},"uncommitted":{},"top_contributors":[{}]}}"#,
            json_escape(&self.branch),
            self.commits,
            self.authors,
            json_escape(&self.first_commit),
            json_escape(&self.last_commit),
            self.active_days,
            self.uncommitted,
            authors.join(",")
        )
    }
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
