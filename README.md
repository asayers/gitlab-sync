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
