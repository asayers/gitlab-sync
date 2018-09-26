#[macro_use]
extern crate structopt;
#[macro_use]
extern crate failure;
extern crate git2;
extern crate gitlab;
#[macro_use]
extern crate failure_derive;
extern crate env_logger;
#[macro_use]
extern crate log;

use git2::{Commit, Repository, Signature, Time};
use gitlab::{Gitlab, MergeRequest, MergeRequestState, MergeRequestStateFilter, ProjectId};
use std::collections::BTreeSet;
use std::process::Command;
use structopt::StructOpt;

type Result<T> = std::result::Result<T, failure::Error>;

const REMOTE: &str = "origin";

#[derive(StructOpt, Debug)]
struct Args {
    /// Download all MRs (default is open only)
    #[structopt(short = "a", long = "--all")]
    all: bool,
    /// Update all MRs (even unchanged)
    #[structopt(short = "f", long = "--force")]
    force: bool,
}

#[derive(Debug, Fail)]
enum Error {
    #[fail(display = "Error deferencing series branch")]
    TargetNotFound(#[cause] git2::Error),
    #[fail(display = "Error deferencing base branch")]
    SourceNotFound(#[cause] git2::Error),
}

fn main() -> Result<()> {
    let args = Args::from_args();
    env_logger::init();
    info!("{:#?}", args);

    let mut repo = Repository::open_from_env()?;
    info!("Connected to local repo at {:?}", repo.path());
    let config = repo.config()?;
    let gitlab_host = config.get_string("gitlab.url")?;
    let gitlab_token = config.get_string("gitlab.privateToken")?;
    let project_id = ProjectId::new(config.get_i64("gitlab.projectId")? as u64);

    let gl = Gitlab::new_insecure(&gitlab_host, gitlab_token).unwrap();
    info!("Connected to gitlab at {}", gitlab_host);

    println!("Fetching from {}", REMOTE);
    git_fetch(REMOTE).unwrap_or_else(|e| error!("{}", e));

    let mrs = if args.all {
        gl.merge_requests(project_id).unwrap()
    } else {
        // TODO: only get MRs which changed since last_update()
        gl.merge_requests_with_state(project_id, MergeRequestStateFilter::Opened)
            .unwrap()
    };
    for mr in mrs {
        let notes = gl.merge_request_notes(project_id, mr.iid).unwrap();
        let mr = MR::new(&mut repo, mr, notes).unwrap();
        import_mr(&args, &mut repo, mr).unwrap_or_else(|e| error!("{}", e));
    }
    Ok(())
}

struct MR {
    source: git2::Oid,
    target: git2::Oid,
    mr: MergeRequest,
    notes: Vec<gitlab::Note>,
}

impl MR {
    fn new(repo: &mut Repository, mr: MergeRequest, notes: Vec<gitlab::Note>) -> Result<MR> {
        // Set base/series gitlinks
        let target = match repo.refname_to_id(&format!("refs/remotes/origin/{}", mr.target_branch))
        {
            Ok(x) => x,
            Err(e) => bail!(Error::TargetNotFound(e)),
        };
        let source = match repo.refname_to_id(&format!("refs/remotes/origin/{}", mr.source_branch))
        {
            Ok(x) => x,
            Err(e) => bail!(Error::SourceNotFound(e)),
        };
        Ok(MR {
            source,
            target,
            mr,
            notes,
        })
    }

    /// Builds a tree and writes it to the repo.  Doesn't modify any refs.
    fn to_tree(&self, repo: &mut Repository) -> Result<git2::Oid> {
        let mut tree = repo.treebuilder(None)?;

        // let base = if repo.graph_descendant_of(source, target).unwrap() {
        //     // it's already merged! let's not update the base.
        //     // TODO
        //     repo.refname_to_id(&format!("{}:base", refname)).unwrap()
        // } else {
        //     repo.merge_base(target, source).unwrap()
        // };
        let base = repo.merge_base(self.target, self.source)?;
        tree.insert("base", base, 0o160000)?;
        tree.insert("series", self.source, 0o160000)?;

        // Handle notes
        let mut notes_tree = repo.treebuilder(None)?;
        let mut ackers = BTreeSet::new();
        if self.mr.user_notes_count > 0 {
            for note in &self.notes {
                if note.system {
                    continue;
                }
                let contents = format!(
                    "From: {} <{}>\nDate: {}\n\n{}\n",
                    note.author.name,
                    lookup_email(&note.author.name)?,
                    note.created_at.to_rfc2822(),
                    note.body
                );
                let blob = repo.blob(contents.as_bytes())?;
                notes_tree.insert(&format!("{}", note.id), blob, 0o100644)?;
                if note.body.contains("LGTM")
                    || note.body.contains("+1")
                    || note.body.contains("Looks good")
                {
                    ackers.insert(note.author.name.clone()); // FIXME: this ^ may be too loose
                }
            }
        }
        tree.insert("notes", notes_tree.write()?, 0o040000)?;

        // Make cover msg
        let mut sections = vec![];
        let mut title = vec![];
        if self.mr.state == MergeRequestState::Closed || self.mr.state == MergeRequestState::Merged
        {
            title.push("[Closed]");
        }
        title.push(&self.mr.title);
        sections.push(title.join(" "));
        if let Some(ref desc) = self.mr.description {
            sections.push(desc.clone());
        }
        sections.push(format!("Closes !{}", self.mr.iid));
        let mut tags = vec![];
        if let Some(ref asgn) = self.mr.assignee {
            if ackers.remove(&asgn.name) {
                tags.push(format!(
                    "Reviewed-by: {} <{}>",
                    asgn.name,
                    lookup_email(&asgn.name)?
                ));
            } else {
                tags.push(format!(
                    "Assigned-to: {} <{}>",
                    asgn.name,
                    lookup_email(&asgn.name)?
                ));
            }
        }
        for acker in ackers {
            tags.push(format!("Acked-by: {} <{}>", acker, lookup_email(&acker)?));
        }
        for asgn in self.mr.assignees.iter().flat_map(|x| x) {
            tags.push(format!("Cc: {} <{}>", asgn.name, lookup_email(&asgn.name)?));
        }
        if !tags.is_empty() {
            sections.push(tags.join("\n"));
        }
        let cover_msg = sections.join("\n\n") + "\n";
        let blob = repo.blob(cover_msg.as_bytes())?;
        tree.insert("cover", blob, 0o100644)?;

        // write!
        Ok(tree.write()?)
    }
}

fn import_mr(args: &Args, repo: &mut Repository, mr: MR) -> Result<()> {
    let tree_oid = mr.to_tree(repo)?;
    let tree_ref = repo.find_tree(tree_oid)?;
    let ts = Time::new(mr.mr.updated_at.timestamp(), 0);
    let author_sig = Signature::new(
        format_name(&mr.mr.author.name),
        format_name(&lookup_email(&mr.mr.author.name)?),
        &ts,
    )?;
    let committer_sig = repo.signature()?;
    let msg = format!("Import latest version of !{}", mr.mr.iid);
    let parent_hack = repo.find_commit(mr.source)?;
    let refname = format!("refs/heads/git-series/gitlab/{}", mr.mr.iid);
    match refname_to_commit(&repo, &refname)? {
        None => {
            repo.commit(
                Some(&refname),
                &author_sig,
                &committer_sig,
                &msg,
                &tree_ref,
                &[&parent_hack],
            )?;
            println!("Created !{}", mr.mr.iid);
        }
        Some(ref parent) if parent.tree_id() == tree_oid && !args.force => {
            info!("!{} already up-to-date", mr.mr.iid);
        }
        Some(parent_real) => {
            repo.commit(
                Some(&refname),
                &author_sig,
                &committer_sig,
                &msg,
                &tree_ref,
                &[&parent_real, &parent_hack],
            )?;
            println!("Updated !{}", mr.mr.iid);
        }
    }
    Ok(())
}

fn refname_to_commit<'a>(repo: &'a Repository, refname: &str) -> Result<Option<Commit<'a>>> {
    Ok(match repo.refname_to_id(refname) {
        Ok(oid) => Some(repo.find_commit(oid)?),
        Err(_) => None,
    })
}

fn git_fetch(remote: &str) -> Result<()> {
    Command::new("git").args(&["fetch", remote]).spawn()?;
    Ok(())
}

fn lookup_email(author: &str) -> Result<String> {
    String::from_utf8(
        Command::new("git")
            .args(&[
                "log",
                "-1",
                "--pretty=%aE",
                "--regexp-ignore-case",
                &format!("--author={}", author),
                "master",
            ]).output()?
            .stdout,
    ).map(|x| x.trim().to_string())
    .map_err(|e| e.into())
}

fn format_name(name: &str) -> &str {
    let foo = name.trim();
    if foo.is_empty() {
        "UNKNOWN"
    } else {
        foo
    }
}

// fn last_update() -> String {
//     String::from_utf8(Command::new("git").args(
//         &["for-each-ref",
//           "refs/heads/git-series",
//           "--format='%(authordate)'",
//           "--sort='-authordate'"]
//         ).output().unwrap().stdout).unwrap()
// }
