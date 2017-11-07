Your repo's config file (.git/config) must contain the following section:

```
[gitlab]
    projectId = 42
    url = gitlab.mydomain.com
    privateToken = "xyz123abc456pqr789st"
```

The working directory must be within a checkout of the specified project.

Open MRs are imported as [git series].

[git series]: https://github.com/git-series/git-series


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
