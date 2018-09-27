# gitlab-sync

Imports gitlab MRs into the local repository using the [git series] format.

[git series]: https://github.com/git-series/git-series

## Setup

Start by adding a "gitlab" section to your repo's config file (.git/config):

```
[gitlab]
    projectId = 42
    url = gitlab.mydomain.com
    privateToken = "xyz123abc456pqr789st"
```

You can get a private token from http://gitlab.mydomain.com/profile/account if
you don't have one yet.

## Usage

The working directory must be within a checkout of the specified project. If
`gitlab-sync` can't find the branches referenced by an MR, it will fail to
import it.

Each MR is imported as a git series. If a series already exists, it is updated.
If you run `gitlab-sync` regularly you get the complete history of all MRs.

By default, only open MRs are imported (override with `-a`).

If `gitlab-sync` fails while communicating with gitlab, it may be due to a
gitlab version incompatibility.  You can fix this by changing the version of
the `gitlab` package in Cargo.toml so that it matches your version of gitlab.
(See [here][gitlab versioning] for more information.)

[gitlab versioning]: https://gitlab.kitware.com/utils/rust-gitlab#versioning

## I imported some MRs; now what?

The MRs are imported in the format understood by `git-series`, so you could use
that; but these days (git-2.19+) you don't even need to have `git-series`
installed to view the ddiffs - it's supported in the base distribution!

Suppose you reviewed MR !2572 last week, but the author has pushed 13 times
since then.  To see the changes since your last review, simply:

```
$ SERIES=git-series/gitlab/2572
$ git range-diff ${SERIES}~13:base..${SERIES}~13:series ${SERIES}:base..${SERIES}:series
```

Try it!  I'm pretty sure this is the best way to re-review a branch.

## License

This is free and unencumbered software released into the public domain.  See
[UNLICENSE](UNLICENSE) or http://unlicense.org/.
