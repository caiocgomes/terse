# Citations and references

Terse never fetches network data during `check`, `build`, `fmt`, `watch`,
or `export`. Reference resolution is one explicit, transactional command.

## Declaring and citing

```
refs:
  turing1936: doi:10.1112/plms/s2-42.1.230
  shannon1948: arxiv:cs/0605016

As shown by @turing1936, ... Some prior work @turing1936[p. 230] found...
```

`refs:` declarations live in one project-wide alias namespace (duplicates,
even for identical identifiers, fail). Only `doi:` and `arxiv:` providers
are supported; anything else (`isbn:`, `url:`, etc.) is a reserved-provider
error at declaration time, whether or not it is cited.

## Resolving

```sh
terse refs resolve            # fetch new/changed aliases, keep the rest
terse refs resolve --refresh  # bypass the cache, re-fetch everything
terse refs resolve --offline  # reseal against existing records only, no network
terse refs resolve --prune    # also drop lock entries no longer declared
```

`refs resolve` is the *only* command that ever constructs a network
transport. It stages every requested record, validates the complete new
lock, and only then atomically replaces `references.lock` — a failure on
any single alias leaves the previous lock completely untouched
(`test_resolution_is_all_or_nothing`).

## Overrides

`references.overrides.toml` lets you patch resolved metadata offline (fix
an author name, add a missing field) without needing a new fetch. An
override is "sealed" against the exact effective record it was written
against; if the underlying resolved record changes, the override must be
re-verified (`--offline` can reseal it) rather than silently reapplied.

## Bibliography

Only cited works are serialized to `.bib`. Citations render as distinct
narrative (`@alias`) or parenthetical/grouped forms, with per-work locators
preserved individually. First-citation order in the running text is
independent of the (key-sorted) `.bib` file itself.
