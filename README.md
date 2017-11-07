Your repo's config file (.git/config) must contain the following section:

```
[gitlab]
    projectId = 42
    url = gitlab.mydomain.com
    privateToken = "xyz123abc456pqr789st"
```

You can get your private token from http://gitlab.mydomain.com/profile/account

The working directory must be within a checkout of the specified project. If
`gitlab-sync` can't find the branches referenced by an MR, it will fail to
import it.

MRs are imported as [git series]. If a series already exists, it is updated. If
you run `gitlab-sync` regularly you get the complete history of an MR.

[git series]: https://github.com/git-series/git-series

By default, only open MRs are imported (override with `-a`).

If `gitlab-sync` fails while communicating with gitlab, you may need to change
the version of the `gitlab` package in Cargo.toml so that it matches your
version of gitlab. (See [here][gitlab versioning] for more information.)

[gitlab versioning]: https://gitlab.kitware.com/utils/rust-gitlab#versioning

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
