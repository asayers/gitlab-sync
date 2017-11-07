extern crate gitlab;
extern crate git2;
extern crate clap;
extern crate loggerv;
#[macro_use] extern crate log;

use std::process::Command;
use std::collections::BTreeSet;
use gitlab::{Gitlab, ProjectId, MergeRequestStateFilter, MergeRequestState, MergeRequest};
use git2::{Repository, Commit, Time, Signature};
use clap::{Arg, App, ArgMatches};

const REMOTE: &str = "origin";

fn main() {
    let args = App::new("app")
            .arg(Arg::with_name("v").short("v").multiple(true).help("Sets the level of verbosity"))
            .arg(Arg::with_name("all").short("a").help("Download all MRs (default is open only)"))
            .arg(Arg::with_name("force").short("f").help("Update all MRs (even unchanged)"))
            .get_matches();

    loggerv::init_with_verbosity(args.occurrences_of("v")).unwrap();

    let mut repo = Repository::open_from_env().unwrap();
    info!("Connected to local repo at {:?}", repo.path());
    let config = repo.config().unwrap();
    let gitlab_host = config.get_string("gitlab.url").unwrap();
    let gitlab_token = config.get_string("gitlab.privateToken").unwrap();
    let project_id = ProjectId::new(config.get_i64("gitlab.projectId").unwrap() as u64);

    let gl = Gitlab::new_insecure(&gitlab_host, gitlab_token).unwrap();
    info!("Connected to gitlab at {}", gitlab_host);

    println!("Fetching from {}", REMOTE);
    git_fetch(REMOTE);

    let mrs = if args.is_present("all") {
        gl.merge_requests(project_id).unwrap()
    } else {
        // TODO: only get MRs which changed since last_update()
        gl.merge_requests_with_state(project_id, MergeRequestStateFilter::Opened).unwrap()
    };
    for mr in mrs {
        import_mr(&args, &gl, project_id, &mut repo, mr);
    }
}

fn import_mr(args: &ArgMatches, gl: &Gitlab, project_id: ProjectId, repo: &mut Repository, mr: MergeRequest) {
    let refname = format!("refs/heads/git-series/gitlab/{}", mr.iid);
    let mut tree = repo.treebuilder(None).unwrap();

    // Set base/series gitlinks
    let target = match repo.refname_to_id(&format!("refs/remotes/origin/{}", mr.target_branch))
                            { Ok(x) => x, Err(e) => { error!("{:?}", e); return (); } };
    let source = match repo.refname_to_id(&format!("refs/remotes/origin/{}", mr.source_branch))
                            { Ok(x) => x, Err(e) => { error!("{:?}", e); return (); } };
    let base = if repo.graph_descendant_of(source, target).unwrap() {
        // it's already merged! let's not update the base.
        // TODO
        repo.refname_to_id(&format!("{}:base", refname)).unwrap()
    } else {
        repo.merge_base(target, source).unwrap()
    };
    tree.insert("base",   base  , 0o160000).unwrap();
    tree.insert("series", source, 0o160000).unwrap();

    // Handle notes
    let mut notes_tree = repo.treebuilder(None).unwrap();
    let mut ackers = BTreeSet::new();
    if mr.user_notes_count > 0 {
        for note in gl.merge_request_notes(project_id, mr.id).unwrap() {
            if note.system { continue; }
            let contents = format!("From: {} <{}>\nDate: {}\n\n{}\n", note.author.name, lookup_email(&note.author.name).trim(), note.created_at.to_rfc2822(), note.body);
            let blob = repo.blob(contents.as_bytes()).unwrap();
            notes_tree.insert(&format!("{}", note.id), blob, 0o100644).unwrap();
            if note.upvote || note.body.contains("LGTM") ||
                              note.body.contains("+1") ||
                              note.body.contains("Looks good") {
                ackers.insert(note.author.name);  // FIXME: this ^ may be too loose
            }
        }
    }
    tree.insert("notes", notes_tree.write().unwrap(), 0o040000).unwrap();

    // Make cover msg
    let mut sections = vec![];
    let mut title = vec![];
    if mr.state == MergeRequestState::Closed || mr.state == MergeRequestState::Merged {
        title.push("[Closed]");
    }
    title.push(&mr.title);
    sections.push(title.join(" "));
    if let Some(desc) = mr.description { sections.push(desc); }
    sections.push(format!("Closes !{}", mr.iid));
    let mut tags = vec![];
    if let Some(asgn) = mr.assignee {
        if ackers.remove(&asgn.name) {
            tags.push(format!("Reviewed-by: {} <{}>", asgn.name, lookup_email(&asgn.name).trim()));
        } else {
            tags.push(format!("Assigned-to: {} <{}>", asgn.name, lookup_email(&asgn.name).trim()));
        }
    }
    for acker in ackers {
        tags.push(format!("Acked-by: {} <{}>", acker, lookup_email(&acker).trim()));
    }
    if !tags.is_empty() { sections.push(tags.join("\n")); }
    let cover_msg = sections.join("\n\n") + "\n";
    let blob = repo.blob(cover_msg.as_bytes()).unwrap();
    tree.insert("cover", blob, 0o100644).unwrap();

    // commit!
    let tree_oid = tree.write().unwrap();
    let tree_ref = repo.find_tree(tree_oid).unwrap();
    let ts = Time::new(mr.updated_at.timestamp(), 0);
    let author_sig = Signature::new(&mr.author.name, lookup_email(&mr.author.name).trim(), &ts).unwrap();
    let committer_sig = repo.signature().unwrap();
    let msg = format!("Import latest version of !{}", mr.iid);
    let parent_hack = repo.find_commit(source).unwrap();
    match refname_to_commit(&repo, &refname).unwrap() {
        None => {
            repo.commit(Some(&refname), &author_sig, &committer_sig, &msg, &tree_ref,
                &[&parent_hack]).unwrap();
            println!("Created !{}", mr.iid);
        }
        Some(ref parent) if parent.tree_id() == tree_oid && !args.is_present("force") => {
            info!("!{} already up-to-date", mr.iid);
        }
        Some(parent_real) => {
            repo.commit(Some(&refname), &author_sig, &committer_sig, &msg, &tree_ref,
                &[&parent_real, &parent_hack]).unwrap();
            println!("Updated !{}", mr.iid);
        }
    }
}

fn refname_to_commit<'a>(repo: &'a Repository, refname: &str) -> Result<Option<Commit<'a>>, git2::Error> {
    Ok(match repo.refname_to_id(refname) {
        Ok(oid) => Some(repo.find_commit(oid)?),
        Err(_) => None,
    })
}

fn git_fetch(remote: &str) {
    Command::new("git").args(&["fetch", remote]).spawn().unwrap();
}

fn lookup_email(author: &str) -> String {
    String::from_utf8(Command::new("git").args(
        &["log",
          "-1",
          "--pretty=%aE",
          "--regexp-ignore-case",
          &format!("--author={}", author),
          "master"]
        ).output().unwrap().stdout).unwrap()
}

fn last_update() -> String {
    String::from_utf8(Command::new("git").args(
        &["for-each-ref",
          "refs/heads/git-series",
          "--format='%(authordate)'",
          "--sort='-authordate'"]
        ).output().unwrap().stdout).unwrap()
}
